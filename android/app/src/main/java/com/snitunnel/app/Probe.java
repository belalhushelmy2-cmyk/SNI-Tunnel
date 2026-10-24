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
        throw new Exception("Both HTTPS providers failed: "+(last==null?"unknown error":last.getClass().getSimpleName()+": "+last.getMessage()),last);
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
    /** Exercise the same SOCKS UDP -> tunneled TCP DNS route used by app DNS. */
    static void verifyDns(int port,int timeout) throws Exception {
        try(Socket control=new Socket();java.net.DatagramSocket udp=new java.net.DatagramSocket(new InetSocketAddress("127.0.0.1",0))) {
            control.connect(new InetSocketAddress("127.0.0.1",port),timeout*1000);control.setSoTimeout(timeout*1000);udp.setSoTimeout(timeout*1000);
            DataInputStream in=new DataInputStream(control.getInputStream());OutputStream out=control.getOutputStream();
            out.write(new byte[]{5,1,0});out.flush();if(in.readUnsignedByte()!=5||in.readUnsignedByte()!=0)throw new Exception("DNS SOCKS authentication failed");
            int local=udp.getLocalPort();out.write(new byte[]{5,3,0,1,127,0,0,1,(byte)(local>>8),(byte)local});out.flush();
            if(in.readUnsignedByte()!=5||in.readUnsignedByte()!=0||in.readUnsignedByte()!=0)throw new Exception("DNS UDP association failed");
            int type=in.readUnsignedByte(),size=type==1?4:type==4?16:type==3?in.readUnsignedByte():-1;
            if(size<0)throw new Exception("Invalid DNS relay address");byte[] address=new byte[size];in.readFully(address);int relayPort=in.readUnsignedShort();
            if(relayPort==0)throw new Exception("Invalid DNS relay port");
            int id=new java.security.SecureRandom().nextInt(65536);
            byte[] query=new byte[]{(byte)(id>>8),(byte)id,1,0,0,1,0,0,0,0,0,0,7,'e','x','a','m','p','l','e',3,'c','o','m',0,0,1,0,1};
            byte[] packet=new byte[10+query.length];packet[3]=1;packet[4]=1;packet[5]=1;packet[6]=1;packet[7]=1;packet[9]=53;System.arraycopy(query,0,packet,10,query.length);
            udp.send(new java.net.DatagramPacket(packet,packet.length,new InetSocketAddress("127.0.0.1",relayPort)));
            byte[] response=new byte[4096];java.net.DatagramPacket reply=new java.net.DatagramPacket(response,response.length);udp.receive(reply);
            validateDnsReply(response,reply.getLength(),id);
        }catch(Exception e){throw new Exception("Tunneled DNS failed: "+e.getClass().getSimpleName()+": "+e.getMessage(),e);}
    }
    static void validateDnsReply(byte[] bytes,int length,int id) throws Exception {
        if(length<4||bytes[0]!=0||bytes[1]!=0||bytes[2]!=0)throw new Exception("Invalid SOCKS DNS datagram");
        int type=bytes[3]&255,offset=type==1?10:type==4?22:type==3&&length>=5?7+(bytes[4]&255):-1;
        if(offset<0||length<offset+12)throw new Exception("Incomplete DNS response");
        if(((bytes[offset]&255)<<8|(bytes[offset+1]&255))!=id||(bytes[offset+2]&128)==0||(bytes[offset+3]&15)!=0||(bytes[offset+6]==0&&bytes[offset+7]==0))throw new Exception("DNS response contains no successful matching answer");
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
