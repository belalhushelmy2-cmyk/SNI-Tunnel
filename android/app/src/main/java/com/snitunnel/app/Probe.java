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
    static long verify(int port) throws Exception {
        long start=System.nanoTime();
        Socket socket=new Socket();
        try {
            socket.connect(new InetSocketAddress("127.0.0.1",port),4000);socket.setSoTimeout(4000);
            handshake(socket,"api.ipify.org",443);
            SSLSocket tls=(SSLSocket)((SSLSocketFactory)SSLSocketFactory.getDefault()).createSocket(socket,"api.ipify.org",443,true);
            try {
                tls.setSoTimeout(4000);
                SSLParameters params=tls.getSSLParameters();params.setEndpointIdentificationAlgorithm("HTTPS");tls.setSSLParameters(params);tls.startHandshake();
                tls.getOutputStream().write("GET / HTTP/1.1\r\nHost: api.ipify.org\r\nConnection: close\r\n\r\n".getBytes(StandardCharsets.US_ASCII));
                DataInputStream input=new DataInputStream(tls.getInputStream());
                StringBuilder first=new StringBuilder();
                while(first.length()<256) { int c=input.read();if(c<0)throw new Exception("Empty health response");first.append((char)c);if(c=='\n')break; }
                if(!first.toString().startsWith("HTTP/1.1 200")&&!first.toString().startsWith("HTTP/1.0 200"))throw new Exception("Health provider refused request");
                return (System.nanoTime()-start)/1000000;
            } finally { tls.close(); }
        } finally { socket.close(); }
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
