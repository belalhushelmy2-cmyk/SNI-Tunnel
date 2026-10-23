package com.snitunnel.app;

import org.json.JSONObject;
import java.net.*;
import java.security.MessageDigest;
import java.security.cert.X509Certificate;
import java.util.*;
import java.util.concurrent.*;
import javax.net.ssl.*;

/** Desktop-compatible explicit insecure mode: discover an unauthenticated
 * certificate, then constrain Xray to it for this connection only. */
final class CertificatePin {
    static Profile prepare(Profile profile,int timeoutSeconds)throws Exception {
        JSONObject value=profile.value;
        if(!value.optString("tls").equals("tls")||!value.optBoolean("allowInsecure",false)||!value.optString("pinnedPeerCertSha256").trim().isEmpty())return profile;
        return discover(profile,timeoutSeconds,CertificatePin::probe);
    }
    interface Attempt {String probe(Profile profile,int timeoutSeconds,boolean tls12)throws Exception;}
    static Profile discover(Profile profile,int timeoutSeconds,Attempt attempt)throws Exception {
        Exception last=null;
        for(int n=0;n<3;n++){
            boolean tls12=n>=1;
            Events.add("TLS discovery "+(n+1)+"/3 · "+(tls12?"TLS 1.2 fallback":"TLS 1.2/1.3"));
            try {
                String pin=attempt.probe(profile,Math.max(2,Math.min(10,timeoutSeconds)),tls12);
                if(!pin.matches("(?i)[0-9a-f]{64}"))throw new Exception("Server supplied no readable leaf certificate");
                return new Profile(new JSONObject(profile.value.toString()).put("pinnedPeerCertSha256",pin).put("_sessionTls12",tls12));
            }catch(Exception e){last=e;Events.add("TLS discovery attempt failed: "+e.getClass().getSimpleName()+": "+e.getMessage());}
            if(n<2)Thread.sleep(300);
        }
        throw new SSLHandshakeException("TLS discovery failed before core startup after 3 attempts: "+(last==null?"unknown error":last.getMessage()));
    }
    private static String probe(Profile profile,int timeoutSeconds,boolean tls12)throws Exception {
        JSONObject value=profile.value;
        String sni=value.optString("sni");if(sni.isEmpty())sni=value.optString("host");if(sni.isEmpty())sni=value.getString("add");
        final X509Certificate[][] presented=new X509Certificate[1][];
        SSLContext context=SSLContext.getInstance("TLS");
        context.init(null,new TrustManager[]{new X509TrustManager(){
            public X509Certificate[] getAcceptedIssuers(){return new X509Certificate[0];}
            public void checkClientTrusted(X509Certificate[] chain,String authType){}
            public void checkServerTrusted(X509Certificate[] chain,String authType){presented[0]=chain.clone();}
        }},null);
        int timeout=Math.max(2,Math.min(30,timeoutSeconds))*1000;
        ScheduledExecutorService timer=Executors.newSingleThreadScheduledExecutor();
        try(Socket tcp=new Socket()) {
            ScheduledFuture<?> deadline=timer.schedule(()->{try{tcp.close();}catch(Exception ignored){}},timeout,TimeUnit.MILLISECONDS);
            try {
                tcp.connect(new InetSocketAddress(value.getString("add"),value.getInt("port")),timeout);
                try(SSLSocket tls=(SSLSocket)context.getSocketFactory().createSocket(tcp,sni,value.getInt("port"),true)) {
                    tls.setSoTimeout(timeout);SSLParameters parameters=tls.getSSLParameters();
                    try{parameters.setServerNames(Collections.singletonList(new SNIHostName(sni)));}catch(IllegalArgumentException ignored){} // IP literals have no SNI.
                    String alpn=value.optString("alpn").trim();if(value.optString("net").equals("ws"))alpn="http/1.1";else if(alpn.isEmpty())alpn=value.optString("net").equals("grpc")?"h2":"h2,http/1.1";
                    if(!alpn.trim().isEmpty())parameters.setApplicationProtocols(Arrays.stream(alpn.split(",")).map(String::trim).filter(n->!n.isEmpty()).toArray(String[]::new));
                    tls.setSSLParameters(parameters);if(tls12)tls.setEnabledProtocols(new String[]{"TLSv1.2"});try{tls.startHandshake();}catch(SSLException e){
                        if(presented[0]==null||presented[0].length==0)throw e;
                        Events.add("TLS discovery received a certificate before handshake closure; trying it with Xray.");
                    }
                    java.security.cert.Certificate leaf=presented[0]!=null&&presented[0].length>0?presented[0][0]:tls.getSession().getPeerCertificates()[0];
                    byte[] digest=MessageDigest.getInstance("SHA-256").digest(leaf.getEncoded());
                    StringBuilder pin=new StringBuilder();for(byte b:digest)pin.append(String.format(Locale.ROOT,"%02x",b&255));
                    return pin.toString();
                }
            }finally{deadline.cancel(false);}
        }finally{timer.shutdownNow();}
    }
}
