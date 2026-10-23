package com.snitunnel.app;

import android.content.Context;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;
import org.json.JSONArray;
import java.security.KeyStore;
import java.util.ArrayList;
import java.util.List;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

final class ProfileStore {
    private final Context context;
    ProfileStore(Context c) { context=c; }
    private SecretKey key() throws Exception {
        KeyStore ks=KeyStore.getInstance("AndroidKeyStore");ks.load(null);
        String alias=context.getPackageName()+".profiles";
        if (!ks.containsAlias(alias)) {
            KeyGenerator generator=KeyGenerator.getInstance("AES","AndroidKeyStore");
            generator.init(new KeyGenParameterSpec.Builder(alias,KeyProperties.PURPOSE_ENCRYPT|KeyProperties.PURPOSE_DECRYPT).setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build());
            generator.generateKey();
        }
        return (SecretKey)ks.getKey(alias,null);
    }
    List<Profile> load() throws Exception {
        String stored=context.getSharedPreferences("profiles",0).getString("encrypted","");
        List<Profile> profiles=new ArrayList<>();if(stored.isEmpty())return profiles;
        String[] parts=stored.split(":");
        Cipher cipher=Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.DECRYPT_MODE,key(),new GCMParameterSpec(128,Base64.decode(parts[0],Base64.NO_WRAP)));
        JSONArray array=new JSONArray(new String(cipher.doFinal(Base64.decode(parts[1],Base64.NO_WRAP)),java.nio.charset.StandardCharsets.UTF_8));
        for(int n=0;n<array.length();n++)profiles.add(new Profile(array.getJSONObject(n)));
        return profiles;
    }
    void save(List<Profile> profiles) throws Exception {
        JSONArray array=new JSONArray();for(Profile p:profiles)array.put(p.value);
        Cipher cipher=Cipher.getInstance("AES/GCM/NoPadding");cipher.init(Cipher.ENCRYPT_MODE,key());
        byte[] encrypted=cipher.doFinal(array.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
        String stored=Base64.encodeToString(cipher.getIV(),Base64.NO_WRAP)+":"+Base64.encodeToString(encrypted,Base64.NO_WRAP);
        if(!context.getSharedPreferences("profiles",0).edit().putString("encrypted",stored).commit())throw new Exception("Could not save profiles");
    }
    String selected() { return context.getSharedPreferences("profiles",0).getString("selected",""); }
    void select(String id) { context.getSharedPreferences("profiles",0).edit().putString("selected",id).apply(); }
    Profile current() throws Exception { for(Profile p:load())if(p.id().equals(selected()))return p;throw new Exception("Select a server first"); }
}
