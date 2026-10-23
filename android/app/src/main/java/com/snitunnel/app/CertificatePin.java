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
        String sni=value.optString("sni");if(sni.isEmpty())sni=value.optString("host");if(sni.isEmpty())sni=value.getString("add");
        SSLContext context=SSLContext.getInstance("TLS");
        context.init(null,new TrustManager[]{new X509TrustManager(){
            public X509Certificate[] getAcceptedIssuers(){return new X509Certificate[0];}
            public void checkClientTrusted(X509Certificate[] chain,String authType){}
            public void checkServerTrusted(X509Certificate[] chain,String authType){}
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
                    String alpn=value.optString("net").equals("ws")?"http/1.1":value.optString("alpn");
                    if(!alpn.trim().isEmpty())parameters.setApplicationProtocols(Arrays.stream(alpn.split(",")).map(String::trim).filter(n->!n.isEmpty()).toArray(String[]::new));
                    tls.setSSLParameters(parameters);tls.startHandshake();
                    byte[] digest=MessageDigest.getInstance("SHA-256").digest(tls.getSession().getPeerCertificates()[0].getEncoded());
                    StringBuilder pin=new StringBuilder();for(byte b:digest)pin.append(String.format(Locale.ROOT,"%02x",b&255));
                    return new Profile(new JSONObject(value.toString()).put("pinnedPeerCertSha256",pin.toString()));
                }
            }finally{deadline.cancel(false);}
        }finally{timer.shutdownNow();}
    }
}
