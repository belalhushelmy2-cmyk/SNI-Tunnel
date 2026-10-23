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
import java.io.File;
import java.io.FileOutputStream;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;
import hev.htproxy.TProxyService;

public final class TunnelService extends VpnService {
    public static volatile String status="Disconnected";
    public static volatile boolean active=false;
    public static volatile long latency=-1;
    public static volatile long uploaded=0,downloaded=0;
    private final ScheduledExecutorService worker=Executors.newSingleThreadScheduledExecutor();
    private volatile boolean stopping=true;
    private Process core;
    private ParcelFileDescriptor tun;
    private ScheduledFuture<?> monitor;
    private int port;
    private int failures;
    private File config;
    @Override public int onStartCommand(Intent intent,int flags,int startId) {
        if(intent!=null&&"STOP".equals(intent.getAction())) { stopping=true;worker.execute(this::finish);return START_NOT_STICKY; }
        if(active)return START_NOT_STICKY;
        stopping=false;active=true;status="Preparing VPN…";
        try { foreground(); } catch(Exception e) {active=false;stopping=true;stopSelf();return START_NOT_STICKY;}
        worker.execute(this::connect);
        return START_NOT_STICKY;
    }
    private void foreground() {
        NotificationManager manager=getSystemService(NotificationManager.class);
        manager.createNotificationChannel(new NotificationChannel("vpn","VPN connection",NotificationManager.IMPORTANCE_LOW));
        PendingIntent open=PendingIntent.getActivity(this,0,new Intent(this,MainActivity.class),PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_UPDATE_CURRENT);
        PendingIntent stop=PendingIntent.getService(this,1,new Intent(this,TunnelService.class).setAction("STOP"),PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_UPDATE_CURRENT);
        Notification notification=new Notification.Builder(this,"vpn").setSmallIcon(R.drawable.ic_shield).setContentTitle(getString(R.string.app_name)).setContentText(status).setContentIntent(open).addAction(new Notification.Action.Builder(null,"Disconnect",stop).build()).setOngoing(true).build();
        if(Build.VERSION.SDK_INT>=34)startForeground(1,notification,ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);else startForeground(1,notification);
    }
    private void connect() {
        try {
            Profile profile=new ProfileStore(this).current();
            try(ServerSocket reserve=new ServerSocket(0,1,java.net.InetAddress.getByName("127.0.0.1"))) { port=reserve.getLocalPort(); }
            config=new File(getFilesDir(),"active-xray.json");write(config,profile.config(port).toString());
            File executable=new File(getApplicationInfo().nativeLibraryDir,"libxray.so");
            if(!executable.canExecute())throw new Exception("Bundled core is not executable on this device");
            core=new ProcessBuilder(executable.getAbsolutePath(),"run","-config",config.getAbsolutePath()).redirectErrorStream(true).redirectOutput(new File("/dev/null")).start();
            boolean ready=false;
            for(int n=0;n<80&&!stopping;n++) {
                if(!core.isAlive())throw new Exception("Core exited before startup");
                try(Socket s=new Socket()) {s.connect(new java.net.InetSocketAddress("127.0.0.1",port),100);ready=true;break;}catch(Exception e){Thread.sleep(100);}
            }
            if(stopping){finish();return;}
            if(!ready)throw new Exception("Core startup timed out");
            Builder builder=new Builder().setSession("SNI Tunnel").setMtu(1500).addAddress("198.18.0.1",30).addAddress("fd00:198:18::1",126).addRoute("0.0.0.0",0).addRoute("::",0).addDnsServer("1.1.1.1");
            // The core subprocess shares this UID. Exclude it to avoid routing loops.
            builder.addDisallowedApplication(getPackageName());
            builder.setMetered(false);
            tun=builder.establish();if(tun==null)throw new Exception("VPN permission was revoked");
            String yaml="tunnel:\n  mtu: 1500\n  ipv4: 198.18.0.1\n  ipv6: 'fd00:198:18::1'\nsocks5:\n  address: 127.0.0.1\n  port: "+port+"\n  udp: 'udp'\nmisc:\n  log-level: error\n  connect-timeout: 5000\n  read-write-timeout: 60000\n";
            File hev=new File(getFilesDir(),"active-tunnel.yml");write(hev,yaml);
            if(!TProxyService.TProxyStartService(hev.getAbsolutePath(),tun.getFd()))throw new Exception("Packet forwarder could not start");
            if(stopping){finish();return;}
            status="VPN active · checking proxy…";foreground();
            failures=0;
            monitor=worker.scheduleWithFixedDelay(this::check,0,30,TimeUnit.SECONDS);
        } catch(Exception|LinkageError error) {
            status="Connection failed. Check the server, TLS SNI and device compatibility.";
            finishKeepingStatus();
        }
    }
    private void check() {
        if(stopping)return;
        try {
            if(core==null||!core.isAlive()||!TProxyService.TProxyIsRunning())throw new Exception("Forwarder stopped");
            latency=Probe.verify(port);failures=0;
            long[] stats=TProxyService.TProxyGetStats();
            if(stats!=null&&stats.length>=4){uploaded=stats[1];downloaded=stats[3];}
            status="VPN active · proxy verified";
        } catch(Exception|LinkageError e) {
            latency=-1;failures++;status="VPN active · health check failed ("+failures+")";
            // Retain the VPN interface while degraded; never silently claim success.
        }
        if(!stopping)foreground();
    }
    private void write(File file,String content)throws Exception {
        try(FileOutputStream out=new FileOutputStream(file)){out.write(content.getBytes(StandardCharsets.UTF_8));}
    }
    private void finish() { status="Disconnected";finishKeepingStatus(); }
    private void finishKeepingStatus() {
        stopping=true;
        if(monitor!=null){monitor.cancel(false);monitor=null;}
        try{TProxyService.TProxyStopService();}catch(LinkageError ignored){}
        if(tun!=null){try{tun.close();}catch(Exception ignored){}tun=null;}
        if(core!=null){core.destroy();try{if(!core.waitFor(2,TimeUnit.SECONDS))core.destroyForcibly();}catch(InterruptedException e){Thread.currentThread().interrupt();}core=null;}
        if(config!=null)config.delete();
        new File(getFilesDir(),"active-tunnel.yml").delete();
        active=false;latency=-1;uploaded=0;downloaded=0;
        stopForeground(STOP_FOREGROUND_REMOVE);stopSelf();
    }
    @Override public void onRevoke(){stopping=true;worker.execute(this::finish);}
    @Override public void onDestroy(){stopping=true;worker.shutdownNow();finishKeepingStatus();super.onDestroy();}
}
