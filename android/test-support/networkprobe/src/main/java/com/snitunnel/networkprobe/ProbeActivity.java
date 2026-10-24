package com.snitunnel.networkprobe;
import android.app.Activity;
import android.os.Bundle;
import java.io.*;
import java.net.*;
import java.security.MessageDigest;
import org.json.JSONObject;
/** Test-only, separately installed UID. Never bundled with phone/TV releases. */
public final class ProbeActivity extends Activity {
 @Override public void onCreate(Bundle state){super.onCreate(state);final String url=getIntent().getStringExtra("url");new Thread(()->{
  JSONObject result=new JSONObject();long begin=System.nanoTime();try{
   URL target=new URL(url);result.put("resolved",InetAddress.getByName(target.getHost()).getHostAddress());
   HttpURLConnection c=(HttpURLConnection)target.openConnection();c.setConnectTimeout(12000);c.setReadTimeout(12000);c.setRequestProperty("Connection","close");
   result.put("status",c.getResponseCode());MessageDigest hash=MessageDigest.getInstance("SHA-256");int count=0,n;
   try(InputStream in=c.getInputStream()){byte[] b=new byte[16384];while((n=in.read(b))>=0){count+=n;if(count>4*1024*1024)throw new IOException("Test download exceeds 4 MiB");hash.update(b,0,n);}}
   c.disconnect();StringBuilder digest=new StringBuilder();for(byte b:hash.digest())digest.append(String.format(java.util.Locale.ROOT,"%02x",b&255));
   result.put("bytes",count).put("sha256",digest.toString()).put("ok",true);
  }catch(Exception e){try{result.put("ok",false).put("error",e.getClass().getSimpleName()+": "+e.getMessage());}catch(Exception ignored){}}
  try{result.put("elapsed_ms",(System.nanoTime()-begin)/1000000);try(FileOutputStream out=openFileOutput("result.json",MODE_PRIVATE)){out.write(result.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));}}catch(Exception ignored){}
  runOnUiThread(this::finish);
 },"vpn-route-test").start();}
}
