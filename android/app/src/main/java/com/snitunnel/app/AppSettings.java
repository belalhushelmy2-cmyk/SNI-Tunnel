package com.snitunnel.app;

import android.content.Context;
import android.content.SharedPreferences;
import org.json.JSONArray;
import org.json.JSONObject;

final class AppSettings {
    final SharedPreferences prefs;
    AppSettings(Context context){prefs=context.getSharedPreferences("settings",0);}
    int interval(){return Math.max(15,Math.min(3600,prefs.getInt("interval",60)));}
    int timeout(){return Math.max(3,Math.min(30,prefs.getInt("timeout",10)));}
    boolean health(){return prefs.getBoolean("health",true);}
    boolean reconnect(){return prefs.getBoolean("reconnect",true);}
    JSONArray sniProfiles(){try{return new JSONArray(prefs.getString("sniProfiles","[]"));}catch(Exception e){return new JSONArray();}}
    void saveSni(JSONArray list)throws Exception{if(!prefs.edit().putString("sniProfiles",list.toString()).commit())throw new Exception("Could not save SNI profiles");}
    Profile apply(Profile server)throws Exception{
        Profile result=new Profile(server.value);String selected=prefs.getString("selectedSni","");
        JSONArray list=sniProfiles();for(int n=0;n<list.length();n++){JSONObject p=list.getJSONObject(n);if(p.optString("id").equals(selected)){result.value.put("sni",p.getString("sni"));break;}}
        return result;
    }
}
