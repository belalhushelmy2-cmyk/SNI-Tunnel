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
}
