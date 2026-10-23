package com.snitunnel.app;

import java.io.DataInputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import javax.net.ssl.SSLParameters;
import javax.net.ssl.SSLSocket;
import javax.net.ssl.SSLSocketFactory;

final class Probe {
    private static final java.util.concurrent.ScheduledExecutorService timer=java.util.concurrent.Executors.newSingleThreadScheduledExecutor(r->{Thread t=new Thread(r,"probe-deadline");t.setDaemon(true);return t;});
    static long verify(int port) throws Exception { return verify(port,10); }
    static long verify(int port,int timeout) throws Exception {
        Exception last=null;
        for(String[] provider:new String[][]{{"api.ipify.org","/"},{"1.1.1.1","/cdn-cgi/trace"}}){
            try{return verifyProvider(port,provider[0],provider[1],timeout);}catch(Exception e){last=e;}
        }
        throw new Exception("Both HTTPS providers failed: "+(last==null?"unknown error":last.getMessage()),last);
    }
    private static long verifyProvider(int port,String host,String path,int timeout) throws Exception {
        long start=System.nanoTime();
        Socket socket=new Socket();
        java.util.concurrent.ScheduledFuture<?> deadline=timer.schedule(()->{try{socket.close();}catch(Exception ignored){}},timeout,java.util.concurrent.TimeUnit.SECONDS);
        try {
            socket.connect(new InetSocketAddress("127.0.0.1",port),timeout*1000);socket.setSoTimeout(timeout*1000);
            handshake(socket,host,443);
            SSLSocket tls=(SSLSocket)((SSLSocketFactory)SSLSocketFactory.getDefault()).createSocket(socket,host,443,true);
            try {
                tls.setSoTimeout(timeout*1000);
                SSLParameters params=tls.getSSLParameters();params.setEndpointIdentificationAlgorithm("HTTPS");tls.setSSLParameters(params);tls.startHandshake();
                tls.getOutputStream().write(("GET "+path+" HTTP/1.1\r\nHost: "+host+"\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.US_ASCII));
                DataInputStream input=new DataInputStream(tls.getInputStream());
                StringBuilder first=new StringBuilder();
                while(first.length()<256) { int c=input.read();if(c<0)throw new Exception("Empty health response");first.append((char)c);if(c=='\n')break; }
                if(!first.toString().startsWith("HTTP/1.1 200")&&!first.toString().startsWith("HTTP/1.0 200"))throw new Exception("Health provider refused request");
                return (System.nanoTime()-start)/1000000;
            } finally { tls.close(); }
        } finally { deadline.cancel(false);socket.close(); }
    }
    static void handshake(Socket socket,String host,int port) throws Exception {
        OutputStream out=socket.getOutputStream();DataInputStream in=new DataInputStream(socket.getInputStream());
        out.write(new byte[]{5,1,0});out.flush();
        if(in.readUnsignedByte()!=5||in.readUnsignedByte()!=0)throw new Exception("SOCKS authentication failed");
        byte[] domain=host.getBytes(StandardCharsets.US_ASCII);
        if(domain.length>255)throw new Exception("Invalid SOCKS destination");
        out.write(new byte[]{5,1,0,3,(byte)domain.length});out.write(domain);out.write(port>>8);out.write(port&255);out.flush();
        if(in.readUnsignedByte()!=5||in.readUnsignedByte()!=0||in.readUnsignedByte()!=0)throw new Exception("SOCKS connection failed");
        int type=in.readUnsignedByte();int length;
        if(type==1)length=4;else if(type==4)length=16;else if(type==3)length=in.readUnsignedByte();else throw new Exception("Invalid SOCKS reply");
        byte[] rest=new byte[length+2];in.readFully(rest);
    }
}
