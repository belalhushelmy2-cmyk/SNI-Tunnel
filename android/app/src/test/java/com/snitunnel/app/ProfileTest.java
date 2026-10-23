package com.snitunnel.app;
import org.junit.Test;
import org.json.JSONObject;
import org.json.JSONArray;
import java.util.Base64;
import java.nio.charset.StandardCharsets;
import java.util.List;
import static org.junit.Assert.*;
public class ProfileTest {
    private JSONObject sample()throws Exception{return new JSONObject().put("add","dial.example.com").put("port",443).put("id","00000000-0000-4000-8000-000000000001").put("net","ws").put("tls","tls").put("sni","tls.example.com").put("host","ws.example.com").put("path","/socket").put("ps","Server");}
    private String link()throws Exception{return "vmess://"+Base64.getEncoder().encodeToString(sample().toString().getBytes(StandardCharsets.UTF_8));}
    @Test public void independentSniHostAddress()throws Exception{JSONObject out=new Profile(sample()).config(1080).getJSONArray("outbounds").getJSONObject(0);assertEquals("dial.example.com",out.getJSONObject("settings").getJSONArray("vnext").getJSONObject(0).getString("address"));assertEquals("tls.example.com",out.getJSONObject("streamSettings").getJSONObject("tlsSettings").getString("serverName"));assertEquals("ws.example.com",out.getJSONObject("streamSettings").getJSONObject("wsSettings").getString("host"));assertFalse(out.toString().contains("allowInsecure"));}
    @Test public void duplicatesRemainSeparate()throws Exception{List<Profile> p=Profile.parse(link()+"\n"+link());assertEquals(2,p.size());assertNotEquals(p.get(0).id(),p.get(1).id());}
    @Test public void subscriptionImport()throws Exception{String data=Base64.getEncoder().encodeToString((link()+"\n"+link()).getBytes(StandardCharsets.UTF_8));assertEquals(2,Profile.parse(data).size());}
    @Test public void jsonArrayImport()throws Exception{assertEquals(2,Profile.parse(new JSONArray().put(sample()).put(sample()).toString()).size());}
    @Test(expected=Exception.class)public void invalidPort()throws Exception{new Profile(sample().put("port",70000));}
    @Test(expected=Exception.class)public void unsupportedTransport()throws Exception{new Profile(sample().put("net","kcp"));}
    @Test(expected=Exception.class)public void invalidUuid()throws Exception{new Profile(sample().put("id","secret"));}
    @Test(expected=Exception.class)public void batchIsAtomic()throws Exception{Profile.parse(link()+"\nnot-a-server");}
}
