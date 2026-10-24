package com.snitunnel.app;
import org.junit.Test;
import org.json.JSONObject;
import static org.junit.Assert.*;
public class TunnelNetworkTest {
 @Test public void dnsOnlyConvertsUdp53AndChainsThroughVmess()throws Exception{
  JSONObject raw=new JSONObject().put("add","dial.example").put("port",443).put("id","00000000-0000-4000-8000-000000000104").put("net","ws").put("path","/socket").put("tls","tls").put("sni","ea.com").put("host","ws.example");
  String before=raw.toString();JSONObject config=new Profile(raw).config(1080);
  JSONObject proxy=config.getJSONArray("outbounds").getJSONObject(0),dns=config.getJSONArray("outbounds").getJSONObject(1),rule=config.getJSONObject("routing").getJSONArray("rules").getJSONObject(0);
  assertEquals("proxy",proxy.getString("tag"));assertEquals("dns",dns.getString("protocol"));assertEquals("proxy",dns.getJSONObject("streamSettings").getJSONObject("sockopt").getString("dialerProxy"));assertEquals("tcp",dns.getJSONObject("settings").getString("rewriteNetwork"));
  assertEquals("direct",dns.getJSONObject("settings").getJSONArray("rules").getJSONObject(0).getString("action")); // DNS action means preserve query; no freedom outbound.
  assertEquals("53",rule.getString("port"));assertEquals("udp",rule.getString("network"));assertEquals("dns-tcp",rule.getString("outboundTag"));
  assertEquals(before,raw.toString());assertFalse(config.toString().contains("freedom"));assertEquals("ea.com",proxy.getJSONObject("streamSettings").getJSONObject("tlsSettings").getString("serverName"));
 }
 private byte[] reply(){byte[] b=new byte[26];b[3]=1;b[10]=0x12;b[11]=0x34;b[12]=(byte)0x81;b[13]=(byte)0x80;b[17]=1;return b;}
 @Test public void matchingDnsAnswerAccepted()throws Exception{Probe.validateDnsReply(reply(),26,0x1234);}
 @Test(expected=Exception.class)public void emptyAnswerRejected()throws Exception{byte[] b=reply();b[17]=0;Probe.validateDnsReply(b,26,0x1234);}
 @Test(expected=Exception.class)public void mismatchedAnswerRejected()throws Exception{Probe.validateDnsReply(reply(),26,0x5678);}
 @Test(expected=Exception.class)public void truncatedDatagramRejected()throws Exception{Probe.validateDnsReply(reply(),12,0x1234);}
 @Test(expected=Exception.class)public void fragmentedDatagramRejected()throws Exception{byte[] b=reply();b[2]=1;Probe.validateDnsReply(b,26,0x1234);}
 @Test public void ratesAndTotalsSurviveRecovery(){TrafficMeter m=new TrafficMeter();m.reset(0);assertArrayEquals(new long[]{100,300,100,300},m.update(100,300,1000));m.newEpoch(1100);assertArrayEquals(new long[]{120,400,20,100},m.update(20,100,2100));assertArrayEquals(new long[]{120,400,0,0},m.update(20,100,3100));}
 @Test public void resetAndSameTimestampNeverProduceInvalidRates(){TrafficMeter m=new TrafficMeter();m.reset(10);assertArrayEquals(new long[]{4,8,0,0},m.update(4,8,10));m.reset(20);assertArrayEquals(new long[]{0,0,0,0},m.update(0,0,20));}
 @Test public void dnsFailureIsClassified(){Events.beginAttempt();Events.add("Tunneled DNS failed: timeout");assertTrue(Events.hint().startsWith("DNS could not"));}
}
