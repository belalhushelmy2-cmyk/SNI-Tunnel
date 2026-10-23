package com.snitunnel.app;

import android.app.Activity;
import android.app.AlertDialog;
import android.app.UiModeManager;
import android.content.ClipboardManager;
import android.content.Intent;
import android.content.res.Configuration;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.graphics.drawable.StateListDrawable;
import android.net.VpnService;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.View;
import android.widget.*;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.List;

public final class MainActivity extends Activity {
    private final Handler handler=new Handler(Looper.getMainLooper());
    private ProfileStore store;
    private List<Profile> profiles=new ArrayList<>();
    private LinearLayout content;
    private Spinner servers;
    private EditText importBox,sni,host;
    private TextView state,traffic,feedback;
    private boolean tv;
    private boolean loading;
    private boolean storeReadable=true;
    private final Runnable tick=new Runnable(){public void run(){
        if(state!=null){state.setText(TunnelService.status);traffic.setText("HTTPS latency: "+(TunnelService.latency<0?"not measured":TunnelService.latency+" ms")+"\nSent "+bytes(TunnelService.uploaded)+"  ·  Received "+bytes(TunnelService.downloaded));}
        handler.postDelayed(this,1000);
    }};
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);
        store=new ProfileStore(this);
        tv=BuildConfig.FLAVOR.equals("tv")||getSystemService(UiModeManager.class).getCurrentModeType()==Configuration.UI_MODE_TYPE_TELEVISION;
        ScrollView scroll=new ScrollView(this);scroll.setFillViewport(true);scroll.setBackgroundColor(Color.rgb(12,17,26));
        content=new LinearLayout(this);content.setOrientation(LinearLayout.VERTICAL);int pad=dp(tv?48:22);content.setPadding(pad,dp(28),pad,dp(32));scroll.addView(content);
        TextView brand=text(tv?"SNI TUNNEL / TV":"SNI TUNNEL",14,0xff66d5cc);brand.setLetterSpacing(0.18f);
        text("Your connection. Your configuration.",tv?28:27,Color.WHITE).setTypeface(null,Typeface.BOLD);
        text(tv?"Use your remote to choose a server and connect.":"A private connection, with your own server.",15,0xff9fb1c9);
        space(16);
        state=text(TunnelService.status,21,0xff66d5cc);state.setPadding(dp(16),dp(18),dp(16),dp(18));state.setBackground(shape(0xff162630,0xff25444c));
        traffic=text("HTTPS latency: not measured",14,0xffa5b8cf);space(10);
        LinearLayout actions=row();
        button(actions,"Connect",v->prepare());button(actions,"Disconnect",v->{if(TunnelService.active)startService(new Intent(this,TunnelService.class).setAction("STOP"));});
        space(16);text("SAVED SERVERS",13,0xff9fb1c9);
        servers=new Spinner(this);servers.setMinimumHeight(dp(52));content.addView(servers);
        servers.setOnItemSelectedListener(new android.widget.AdapterView.OnItemSelectedListener(){
            public void onItemSelected(AdapterView<?> parent,View view,int position,long id){if(!loading&&position<profiles.size()){Profile p=profiles.get(position);store.select(p.id());showFields(p);}}
            public void onNothingSelected(AdapterView<?> parent){}
        });
        sni=input("TLS SNI (independent of dial address)",false);host=input("WebSocket Host (optional)",false);
        LinearLayout settings=row();button(settings,"Save SNI / Host",v->saveFields());button(settings,"Delete server",v->delete());
        space(18);text("ADD YOUR SERVERS",13,0xff9fb1c9);
        importBox=input("Paste vmess:// links, subscription text or VMess JSON",true);importBox.setMinLines(tv?2:3);importBox.setMaxLines(5);
        LinearLayout imports=row();button(imports,"Import text",v->importText(importBox.getText().toString()));button(imports,"Clipboard",v->clipboard());
        button(content,"Import a file",v->{try{startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT).setType("*/*").addCategory(Intent.CATEGORY_OPENABLE),12);}catch(Exception e){message("No file picker is installed. Paste a link instead.");}});
        feedback=text("Imports append profiles and preserve existing servers.",14,0xff9fb1c9);
        space(16);text("Android 10+ · VMess / TCP / WebSocket / gRPC\nTLS certificates are verified. Android asks permission before enabling VPN.\nThis app's own traffic bypasses the VPN to prevent a core routing loop. The health check verifies the proxy, not every other app's traffic.",12,0xff8b9db6);
        setContentView(scroll);reload();
        if(Build.VERSION.SDK_INT>=33&&checkSelfPermission("android.permission.POST_NOTIFICATIONS")!=android.content.pm.PackageManager.PERMISSION_GRANTED)requestPermissions(new String[]{"android.permission.POST_NOTIFICATIONS"},20);
    }
    @Override protected void onResume(){super.onResume();handler.post(tick);}
    @Override protected void onPause(){handler.removeCallbacks(tick);super.onPause();}
    private void reload(){
        try{profiles=store.load();storeReadable=true;}catch(Exception e){storeReadable=false;message("Could not read encrypted profiles. Existing data was retained.");return;}
        loading=true;
        List<String> labels=new ArrayList<>();int selected=0;
        for(int n=0;n<profiles.size();n++){labels.add((n+1)+". "+profiles.get(n).name());if(profiles.get(n).id().equals(store.selected()))selected=n;}
        if(labels.isEmpty())labels.add("No servers yet — import below");
        ArrayAdapter<String> adapter=new ArrayAdapter<>(this,android.R.layout.simple_spinner_item,labels);adapter.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);servers.setAdapter(adapter);servers.setSelection(selected);
        if(!profiles.isEmpty()){store.select(profiles.get(selected).id());showFields(profiles.get(selected));}else{sni.setText("");host.setText("");}
        loading=false;
    }
    private void showFields(Profile p){sni.setText(p.value.optString("sni"));host.setText(p.value.optString("host"));}
    private void saveFields(){
        if(TunnelService.active){message("Disconnect before editing the server.");return;}
        try{Profile current=store.current();for(Profile p:profiles)if(p.id().equals(current.id())){p.value.put("sni",sni.getText().toString().trim());p.value.put("host",host.getText().toString().trim());}store.save(profiles);message("Server settings saved.");}catch(Exception e){message("Select a server and check its settings.");}
    }
    private void importText(String raw){
        if(!storeReadable){message("Saved profiles could not be decrypted. Existing data was retained; import is disabled.");return;}
        try{List<Profile> added=Profile.parse(raw);List<Profile> candidate=new ArrayList<>(profiles);candidate.addAll(added);store.save(candidate);if(profiles.isEmpty())store.select(added.get(0).id());reload();importBox.setText("");message("Added "+added.size()+" server(s); "+candidate.size()+" saved.");}catch(Exception e){message("Import failed. Use valid VMess links or VMess JSON. Nothing was replaced.");}
    }
    private void clipboard(){
        ClipboardManager clipboard=getSystemService(ClipboardManager.class);
        if(!clipboard.hasPrimaryClip()||clipboard.getPrimaryClip()==null||clipboard.getPrimaryClip().getItemCount()==0){message("Clipboard is empty.");return;}
        CharSequence value=clipboard.getPrimaryClip().getItemAt(0).coerceToText(this);if(value!=null)importText(value.toString());
    }
    private void delete(){
        if(TunnelService.active){message("Disconnect before deleting a server.");return;}
        if(profiles.isEmpty())return;
        new AlertDialog.Builder(this).setTitle("Delete selected server?").setMessage("Other servers will be kept.").setNegativeButton("Cancel",null).setPositiveButton("Delete",(d,w)->{
            try{String id=store.selected();List<Profile> candidate=new ArrayList<>();for(Profile p:profiles)if(!p.id().equals(id))candidate.add(p);store.save(candidate);store.select(candidate.isEmpty()?"":candidate.get(0).id());reload();}catch(Exception e){message("Could not delete server. Saved data was retained.");}
        }).show();
    }
    private void prepare(){
        if(TunnelService.active){message("A connection is already active.");return;}
        try{store.current();}catch(Exception e){message("Import and select a server first.");return;}
        Intent permission=VpnService.prepare(this);
        if(permission!=null)startActivityForResult(permission,11);else connect();
    }
    private void connect(){startForegroundService(new Intent(this,TunnelService.class).setAction("CONNECT"));}
    @Override protected void onActivityResult(int request,int result,Intent data){
        super.onActivityResult(request,result,data);
        if(request==11){if(result==RESULT_OK)connect();else message("VPN permission was not granted.");}
        if(request==12&&result==RESULT_OK&&data!=null&&data.getData()!=null){
            final android.net.Uri uri=data.getData();
            new Thread(()->{try(InputStream in=getContentResolver().openInputStream(uri);ByteArrayOutputStream out=new ByteArrayOutputStream()){
                if(in==null)throw new Exception();byte[] buffer=new byte[4096];int count;
                while((count=in.read(buffer))!=-1){if(out.size()+count>1024*1024)throw new Exception();out.write(buffer,0,count);}
                String raw=new String(out.toByteArray(),java.nio.charset.StandardCharsets.UTF_8);runOnUiThread(()->importText(raw));
            }catch(Exception e){runOnUiThread(()->message("Could not read file. Use a UTF-8 file smaller than 1 MB."));}},"profile-import").start();
        }
    }
    private void message(String value){if(feedback!=null)feedback.setText(value);else Toast.makeText(this,value,Toast.LENGTH_LONG).show();}
    private int dp(int value){return Math.round(value*getResources().getDisplayMetrics().density);}
    private TextView text(String value,int size,int color){TextView view=new TextView(this);view.setText(value);view.setTextSize(size);view.setTextColor(color);view.setPadding(0,dp(5),0,dp(5));content.addView(view);return view;}
    private EditText input(String hint,boolean multiline){EditText v=new EditText(this);v.setHint(hint);v.setTextColor(Color.WHITE);v.setHintTextColor(0xff9fb1c9);v.setTextSize(15);v.setPadding(dp(12),dp(12),dp(12),dp(12));v.setBackground(focus());v.setSingleLine(!multiline);v.setInputType(android.text.InputType.TYPE_CLASS_TEXT|android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS|(multiline?android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE:0));LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(-1,-2);p.setMargins(0,dp(6),0,dp(6));v.setLayoutParams(p);content.addView(v);return v;}
    private LinearLayout row(){LinearLayout v=new LinearLayout(this);v.setOrientation(LinearLayout.HORIZONTAL);content.addView(v);return v;}
    private void button(LinearLayout parent,String label,View.OnClickListener action){Button v=new Button(this);v.setText(label);v.setTextSize(tv?16:14);v.setAllCaps(false);v.setTextColor(Color.WHITE);v.setBackground(focus());v.setMinHeight(dp(54));v.setFocusable(true);v.setOnClickListener(action);LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(parent==content?-1:0,-2,parent==content?0:1);p.setMargins(dp(3),dp(5),dp(3),dp(5));parent.addView(v,p);}
    private GradientDrawable shape(int fill,int border){GradientDrawable d=new GradientDrawable();d.setColor(fill);d.setCornerRadius(dp(12));d.setStroke(dp(2),border);return d;}
    private StateListDrawable focus(){StateListDrawable d=new StateListDrawable();d.addState(new int[]{android.R.attr.state_focused},shape(0xff224b58,0xff66d5cc));d.addState(new int[]{android.R.attr.state_pressed},shape(0xff224b58,0xff66d5cc));d.addState(new int[]{},shape(0xff18232f,0xff2b3948));return d;}
    private void space(int height){View v=new View(this);content.addView(v,new LinearLayout.LayoutParams(1,dp(height)));}
    private static String bytes(long value){return value<1024?value+" B":value<1048576?String.format(java.util.Locale.US,"%.1f KB",value/1024.0):String.format(java.util.Locale.US,"%.1f MB",value/1048576.0);}
}
