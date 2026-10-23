package com.snitunnel.app;

import java.text.SimpleDateFormat;
import java.util.ArrayDeque;
import java.util.Date;
import java.util.Locale;

final class Events {
    private static final ArrayDeque<String> lines=new ArrayDeque<>();
    static synchronized void add(String value){
        String clean=clean(value);
        lines.addLast(new SimpleDateFormat("HH:mm:ss",Locale.US).format(new Date())+"  "+clean);
        while(lines.size()>100)lines.removeFirst();
    }
    static String clean(String value){
        if(value==null)return "Unknown error";
        value=value.replaceAll("(?i)vmess://[^\\s]+","[server link hidden]").replaceAll("(?i)[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}","[UUID hidden]");
        return value.length()>1200?value.substring(0,1200):value;
    }
    static synchronized String text(){StringBuilder out=new StringBuilder();for(String line:lines)out.append(line).append('\n');return out.toString();}
    static synchronized void clear(){lines.clear();}
    static synchronized String hint(){
        String all=text().toLowerCase(Locale.US);
        if(all.contains("certificate")||all.contains("x509")||all.contains("handshake_failure"))return "TLS certificate rejected. Check TLS SNI / verification name or use a certificate pin you trust.";
        if(all.contains("bad handshake")||all.contains("500 internal"))return "WebSocket handshake rejected. Check dial address, port, Host and path; the server may be failing.";
        if(all.contains("connection refused"))return "Server refused the connection. Check address and port.";
        if(all.contains("eof")||all.contains("reset by peer"))return "The server closed the connection. Check transport settings and server availability.";
        return "Could not forward HTTPS through the server. Open Activity for the connection details.";
    }
}
