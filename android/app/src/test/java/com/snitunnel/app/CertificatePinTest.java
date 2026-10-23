package com.snitunnel.app;
import org.junit.Test;
import org.json.JSONObject;
import static org.junit.Assert.*;

public class CertificatePinTest {
    private Profile profile()throws Exception{return new Profile(new JSONObject().put("add","invalid.invalid").put("port",443).put("id","00000000-0000-4000-8000-000000000001").put("tls","tls"));}
    @Test public void strictProfilesNeverDiscoverUntrustedCertificates()throws Exception{Profile p=profile();assertSame(p,CertificatePin.prepare(p,2));}
    @Test public void explicitPinsTakePrecedence()throws Exception{Profile p=profile();p.value.put("allowInsecure",true).put("pinnedPeerCertSha256",new String(new char[64]).replace('\0','a'));assertSame(p,CertificatePin.prepare(p,2));}
    @Test public void plaintextNeverDiscoversCertificates()throws Exception{Profile p=profile();p.value.put("allowInsecure",true).put("tls","");assertSame(p,CertificatePin.prepare(p,2));}
    @Test public void insecureCanBeSavedButNotUsedBeforeDiscovery()throws Exception{Profile p=profile();p.value.put("allowInsecure",true);p.validateForSave();assertTrue(p.value.optBoolean("allowInsecure"));assertFalse(p.value.has("pinnedPeerCertSha256"));try{p.config(1080);fail("Unresolved insecure mode must not start");}catch(Exception expected){assertTrue(expected.getMessage().contains("discovery"));}}
    @Test public void discoveryRetriesAndFallsBackWithoutChangingStoredFields()throws Exception{
        Profile p=profile();p.value.put("allowInsecure",true).put("sni","custom.example").put("host","ws.example");final int[] calls={0};
        String pin=new String(new char[64]).replace('\0','b');
        Profile runtime=CertificatePin.discover(p,2,(server,timeout,tls12)->{calls[0]++;assertSame(p,server);assertEquals(2,timeout);if(calls[0]<3){assertEquals(calls[0]>=2,tls12);throw new javax.net.ssl.SSLHandshakeException("connection closed");}assertTrue(tls12);return pin;});
        assertEquals(3,calls[0]);assertFalse(p.value.has("pinnedPeerCertSha256"));assertEquals(pin,runtime.value.getString("pinnedPeerCertSha256"));assertEquals("custom.example",runtime.value.getString("sni"));assertEquals("ws.example",runtime.value.getString("host"));JSONObject tls=runtime.config(1080).getJSONArray("outbounds").getJSONObject(0).getJSONObject("streamSettings").getJSONObject("tlsSettings");assertEquals("1.2",tls.getString("maxVersion"));assertEquals("unsafe",tls.getString("fingerprint"));assertFalse(p.value.has("_sessionTls12"));
    }
    @Test public void failedDiscoveryNeverCreatesAPin()throws Exception{
        Profile p=profile();final int[] calls={0};
        try{CertificatePin.discover(p,2,(server,timeout,tls12)->{calls[0]++;throw new javax.net.ssl.SSLHandshakeException("connection closed");});fail("All attempts failed");}catch(javax.net.ssl.SSLHandshakeException e){assertTrue(e.getMessage().contains("before core startup"));}
        assertEquals(3,calls[0]);assertFalse(p.value.has("pinnedPeerCertSha256"));
    }
}
