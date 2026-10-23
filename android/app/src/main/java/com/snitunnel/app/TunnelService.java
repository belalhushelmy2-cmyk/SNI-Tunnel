package com.snitunnel.app;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.net.VpnService;
import android.os.Build;
import android.os.ParcelFileDescriptor;
import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.*;
import hev.htproxy.TProxyService;

public final class TunnelService extends VpnService {
    public static volatile String status="Disconnected";
    public static volatile boolean active=false,verified=false,verifyRequested=false;
    public static volatile long latency=-1,uploaded=0,downloaded=0,connectedAt=0;
    public static volatile String serverName="No active server";
    private final ScheduledExecutorService worker=Executors.newSingleThreadScheduledExecutor();
    private volatile boolean stopping=true;
    private boolean recoveryExhausted;
    private Process core;
    private ParcelFileDescriptor tun;
    private ScheduledFuture<?> monitor;
    private int port,failures,retries;
    private long lastCheck,firstFailure;
    private File config;
    private Profile profile;
    private AppSettings settings;
    @Override public int onStartCommand(Intent intent,int flags,int startId) {
        if(intent!=null&&"STOP".equals(intent.getAction())){stopping=true;worker.execute(()->finish("Disconnected"));return START_NOT_STICKY;}
        if(intent!=null&&"RECONNECT".equals(intent.getAction())&&active){worker.execute(()->recover(true));return START_NOT_STICKY;}
        if(active)return START_NOT_STICKY;
        Events.beginAttempt();stopping=false;active=true;verified=false;recoveryExhausted=false;settings=new AppSettings(this);update("Preparing connection…");
        try{foreground();}catch(Exception e){finish("Foreground service failed: "+Events.clean(e.getMessage()));return START_NOT_STICKY;}
        worker.execute(this::connect);return START_NOT_STICKY;
    }
    private void update(String value){status=value;Events.add(value);}
    private void foreground(){
        NotificationManager manager=getSystemService(NotificationManager.class);
        manager.createNotificationChannel(new NotificationChannel("vpn","VPN connection",NotificationManager.IMPORTANCE_LOW));
        PendingIntent open=PendingIntent.getActivity(this,0,new Intent(this,MainActivity.class),PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_UPDATE_CURRENT);
        PendingIntent stop=PendingIntent.getService(this,1,new Intent(this,TunnelService.class).setAction("STOP"),PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_UPDATE_CURRENT);
        Notification notification=new Notification.Builder(this,"vpn").setSmallIcon(R.drawable.ic_shield).setContentTitle(getString(R.string.app_name)).setContentText(status).setContentIntent(open).addAction(new Notification.Action.Builder(null,"Disconnect",stop).build()).setOngoing(true).build();
        if(Build.VERSION.SDK_INT>=34)startForeground(1,notification,ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);else startForeground(1,notification);
    }
    private void connect(){
        try{
            profile=settings.apply(new ProfileStore(this).current());serverName=profile.name();
            try(ServerSocket reserve=new ServerSocket(0,1,InetAddress.getByName("127.0.0.1"))){port=reserve.getLocalPort();}
            startCore();
            update("Verifying server forwarding before enabling VPN…");foreground();
            latency=Probe.verify(port,settings.timeout());
            if(stopping){finish("Disconnected");return;}
            Builder builder=new Builder().setSession("SNI Tunnel").setMtu(1500).addAddress("198.18.0.1",30).addAddress("fd00:198:18::1",126).addRoute("0.0.0.0",0).addRoute("::",0).addDnsServer("1.1.1.1");
            builder.addDisallowedApplication(getPackageName());builder.setMetered(false);
            tun=builder.establish();if(tun==null)throw new Exception("VPN permission was revoked");
            startForwarder();
            if(stopping){finish("Disconnected");return;}
            verified=true;connectedAt=System.currentTimeMillis();lastCheck=connectedAt;failures=0;retries=0;
            update("Connected · proxy verified");foreground();
            monitor=worker.scheduleWithFixedDelay(this::tick,1,1,TimeUnit.SECONDS);
        }catch(Exception|LinkageError error){Events.add("Startup: "+error.getClass().getSimpleName()+": "+error.getMessage());finish("Connection failed · "+Events.hint());}
    }
    private void startCore()throws Exception{
        config=new File(getFilesDir(),"active-xray.json");if(profile.value.optString("tls").equals("tls")&&profile.value.optBoolean("allowInsecure",false)&&profile.value.optString("pinnedPeerCertSha256").trim().isEmpty()){update("Discovering server TLS certificate…");foreground();}
        Profile runtime=CertificatePin.prepare(profile,settings.timeout());
        if(profile.value.optBoolean("allowInsecure",false))Events.add("Allow insecure enabled: server identity is not authenticated.");
        write(config,runtime.config(port).toString());
        Events.beginAttempt(); // Keep history, but classify subsequent failures from the core stage only.
        if(runtime.value.optBoolean("_sessionTls12",false))Events.add("TLS 1.2 compatibility: core uses native TLS with the discovered session pin.");
        File executable=new File(getApplicationInfo().nativeLibraryDir,"libxray.so");
        if(!executable.canExecute())throw new Exception("Bundled core is not executable on this device");
        core=new ProcessBuilder(executable.getAbsolutePath(),"run","-config",config.getAbsolutePath()).redirectErrorStream(true).start();
        final Process launched=core;
        Thread logs=new Thread(()->{try(BufferedReader reader=new BufferedReader(new InputStreamReader(launched.getInputStream(),StandardCharsets.UTF_8))){String line;while((line=reader.readLine())!=null)Events.add("Core: "+line);}catch(IOException ignored){}},"core-log");logs.setDaemon(true);logs.start();
        boolean ready=false;
        for(int n=0;n<100&&!stopping;n++){
            if(!core.isAlive())throw new Exception("Core exited before startup");
            try(Socket s=new Socket()){s.connect(new InetSocketAddress("127.0.0.1",port),100);ready=true;break;}catch(IOException e){Thread.sleep(100);}
        }
        if(stopping)throw new InterruptedException("Connection cancelled");
        if(!ready)throw new Exception("Core startup timed out");
    }
    private void startForwarder()throws Exception{
        String yaml="tunnel:\n  mtu: 1500\n  ipv4: 198.18.0.1\n  ipv6: 'fd00:198:18::1'\nsocks5:\n  address: 127.0.0.1\n  port: "+port+"\n  udp: 'udp'\nmisc:\n  log-level: error\n  connect-timeout: 10000\n  read-write-timeout: 60000\n";
        File hev=new File(getFilesDir(),"active-tunnel.yml");write(hev,yaml);
        if(!TProxyService.TProxyStartService(hev.getAbsolutePath(),tun.getFd()))throw new Exception("Packet forwarder could not start");
    }
    private void tick(){
        if(stopping||recoveryExhausted)return;
        try{
            if(core==null||!core.isAlive()||!TProxyService.TProxyIsRunning()){verified=false;recover(false);return;}
            long[] stats=TProxyService.TProxyGetStats();if(stats!=null&&stats.length>=4){uploaded=stats[1];downloaded=stats[3];}
            if(!verifyRequested&&(!settings.health()||System.currentTimeMillis()-lastCheck<settings.interval()*1000L))return;
            verifyRequested=false;lastCheck=System.currentTimeMillis();
            latency=Probe.verify(port,settings.timeout());if(stopping)return;verified=true;failures=0;firstFailure=0;
            if(!status.equals("Connected · proxy verified"))update("Connected · proxy verified");
        }catch(Exception|LinkageError e){
            verified=false;latency=-1;failures++;if(firstFailure==0)firstFailure=System.currentTimeMillis();
            Events.add("Health: "+e.getClass().getSimpleName()+": "+e.getMessage());
            update("Degraded · VPN retained · "+Events.hint());
            if(failures>=3&&System.currentTimeMillis()-firstFailure>=60000)recover(false);
        }
        if(!stopping)foreground();
    }
    private void recover(boolean manual){
        if(stopping)return;
        if(manual)recoveryExhausted=false;
        if(!manual&&!settings.reconnect()){recoveryExhausted=true;verified=false;update("Degraded · recovery disabled; reconnect manually");foreground();return;}
        if(tun==null){finish("Disconnected; press Connect to retry");return;}
        verified=false;
        for(int attempt=1;attempt<=3&&!stopping;attempt++){
            Events.beginAttempt();retries=attempt;update("Reconnecting · attempt "+attempt+"/3; VPN retained");foreground();
            try{
                stopDatapath();
                if(manual)profile=settings.apply(new ProfileStore(this).current());
                for(int n=0;n<(1<<(attempt-1))*10&&!stopping;n++)Thread.sleep(100);
                if(stopping)return;
                startCore();latency=Probe.verify(port,settings.timeout());startForwarder();
                serverName=profile.name();verified=true;failures=0;firstFailure=0;lastCheck=System.currentTimeMillis();
                update("Connected · proxy verified");foreground();return;
            }catch(Exception|LinkageError e){Events.add("Recovery: "+e.getClass().getSimpleName()+": "+e.getMessage());}
        }
        if(!stopping){recoveryExhausted=true;update("Recovery failed · VPN retained; reconnect or disconnect");foreground();}
    }
    private void stopDatapath(){
        try{TProxyService.TProxyStopService();}catch(LinkageError ignored){}
        if(core!=null){core.destroy();try{if(!core.waitFor(2,TimeUnit.SECONDS))core.destroyForcibly();}catch(InterruptedException e){Thread.currentThread().interrupt();}core=null;}
    }
    private void write(File file,String value)throws Exception{try(FileOutputStream out=new FileOutputStream(file)){out.write(value.getBytes(StandardCharsets.UTF_8));}}
    private synchronized void finish(String message){
        stopping=true;verified=false;if(monitor!=null){monitor.cancel(false);monitor=null;}stopDatapath();
        if(tun!=null){try{tun.close();}catch(IOException ignored){}tun=null;}
        if(config!=null)config.delete();new File(getFilesDir(),"active-tunnel.yml").delete();
        active=false;latency=-1;connectedAt=0;uploaded=0;downloaded=0;verifyRequested=false;
        if(!status.equals(message))update(message);stopForeground(STOP_FOREGROUND_REMOVE);stopSelf();
    }
    @Override public void onRevoke(){stopping=true;worker.execute(()->finish("VPN permission revoked"));}
    @Override public void onDestroy(){stopping=true;worker.shutdownNow();finish(status);super.onDestroy();}
}
