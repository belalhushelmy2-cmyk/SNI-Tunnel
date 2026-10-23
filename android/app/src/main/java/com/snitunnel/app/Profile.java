package com.snitunnel.app;

import org.json.JSONArray;
import org.json.JSONObject;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Base64;
import java.util.List;
import java.util.UUID;

public final class Profile {
    public final JSONObject value;
    public Profile(JSONObject data) throws Exception {
        value = new JSONObject(data.toString());
        String address = value.optString("add").trim();
        if (address.isEmpty() || address.contains("\n") || address.contains("/")) throw new Exception("Missing or invalid server address");
        int port = value.optInt("port", 0);
        if (port < 1 || port > 65535) throw new Exception("Server port must be 1–65535");
        UUID.fromString(value.getString("id"));
        String transport = value.optString("net", "tcp");
        if (!transport.equals("tcp") && !transport.equals("ws") && !transport.equals("grpc")) throw new Exception("Supported transports: TCP, WebSocket and gRPC");
        if (value.optInt("aid",0) != 0) throw new Exception("Legacy alterId must be zero");
        String tls = value.optString("tls", "");
        if (!tls.isEmpty() && !tls.equals("none") && !tls.equals("tls")) throw new Exception("Only standard TLS or plain VMess is supported");
        String method = value.optString("scy", "auto");
        if (!java.util.Arrays.asList("auto","aes-128-gcm","chacha20-poly1305","none","zero").contains(method)) throw new Exception("Unsupported VMess encryption");
        if (!value.has("localId")) value.put("localId", UUID.randomUUID().toString());
    }
    public String name() { return value.optString("ps", value.optString("add")); }
    public String id() { return value.optString("localId"); }
    public static List<Profile> parse(String raw) throws Exception {
        raw = raw.trim();
        if (raw.length() > 1024*1024) throw new Exception("Import exceeds 1 MB");
        if (raw.isEmpty()) throw new Exception("Paste a vmess:// link first");
        List<Profile> result = new ArrayList<>();
        if (raw.startsWith("[")) {
            JSONArray array = new JSONArray(raw);
            for (int n=0;n<array.length();n++) result.add(new Profile(array.getJSONObject(n)));
        } else if (raw.startsWith("{")) result.add(new Profile(new JSONObject(raw)));
        else {
            if (!raw.startsWith("vmess://")) raw = decode(raw);
            for (String line : raw.split("\\r?\\n")) {
                if (line.trim().isEmpty()) continue;
                if (!line.trim().startsWith("vmess://")) throw new Exception("Only VMess links and VMess JSON profiles are supported");
                Profile p = new Profile(new JSONObject(decode(line.trim().substring(8))));
                p.value.put("localId", UUID.randomUUID().toString());
                result.add(p);
            }
        }
        if (result.isEmpty()) throw new Exception("No profiles found");
        // Imports always append as distinct profiles, including identical names.
        for (Profile p : result) p.value.put("localId", UUID.randomUUID().toString());
        return result;
    }
    private static String decode(String value) {
        return new String(Base64.getDecoder().decode(value.replace('-','+').replace('_','/').replaceAll("\\s", "")), StandardCharsets.UTF_8);
    }
    public JSONObject config(int port) throws Exception {
        JSONObject v = value;
        JSONObject user = new JSONObject().put("id",v.getString("id")).put("security",v.optString("scy","auto"));
        JSONObject node = new JSONObject().put("address",v.getString("add")).put("port",v.getInt("port")).put("users",new JSONArray().put(user));
        String net=v.optString("net","tcp");
        JSONObject stream = new JSONObject().put("network",net);
        if (v.optString("tls").equals("tls")) {
            String sni=v.optString("sni");
            if (sni.isEmpty()) sni=v.optString("host");
            if (sni.isEmpty()) sni=v.getString("add");
            stream.put("security","tls").put("tlsSettings",new JSONObject().put("serverName",sni));
        }
        if (net.equals("ws")) stream.put("wsSettings",new JSONObject().put("path",v.optString("path","/")).put("host",v.optString("host")));
        if (net.equals("grpc")) stream.put("grpcSettings",new JSONObject().put("serviceName",v.optString("path")));
        JSONObject outbound = new JSONObject().put("protocol","vmess").put("settings",new JSONObject().put("vnext",new JSONArray().put(node))).put("streamSettings",stream);
        JSONObject inbound = new JSONObject().put("listen","127.0.0.1").put("port",port).put("protocol","socks").put("settings",new JSONObject().put("auth","noauth").put("udp",true));
        return new JSONObject().put("log",new JSONObject().put("loglevel","none")).put("inbounds",new JSONArray().put(inbound)).put("outbounds",new JSONArray().put(outbound));
    }
}
