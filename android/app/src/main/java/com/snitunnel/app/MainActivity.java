package com.snitunnel.app;

import android.app.*;
import android.content.*;
import android.content.ClipboardManager;
import android.content.res.Configuration;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.*;
import android.net.VpnService;
import android.os.*;
import android.text.*;
import android.view.*;
import android.widget.*;
import org.json.*;
import java.io.*;
import java.net.*;
import java.util.*;

public final class MainActivity extends Activity {
    private static final String[] PAGES={"Overview","Servers","SNI profiles","Activity","Settings","Diagnostics"};
    private static final int INK=0xffe7edf8,MUTED=0xff9fb1c9,ACCENT=0xff66d5cc;
    private final Handler handler=new Handler(Looper.getMainLooper());
    private final Map<String,Long> latencies=new HashMap<>();
    private ProfileStore store;private AppSettings settings;private List<Profile> profiles=new ArrayList<>();
    private LinearLayout root,content,sidebar,serverList,topBar;private ScrollView scroll;
    private TextView state,traffic,activityLog,feedback;private EditText importBox,search;
    private boolean tv,readable=true,sortLatency;private int page;private String query="";
    private final Runnable tick=new Runnable(){public void run(){refreshStatus();handler.postDelayed(this,1000);}};
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);store=new ProfileStore(this);settings=new AppSettings(this);
        tv=BuildConfig.FLAVOR.equals("tv")||getSystemService(UiModeManager.class).getCurrentModeType()==Configuration.UI_MODE_TYPE_TELEVISION;
        page=saved==null?0:saved.getInt("page",0);load();
        root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);root.setBackgroundColor(0xff0c111a);
        if(!tv){topBar=new LinearLayout(this);topBar.setPadding(dp(12),dp(8),dp(12),dp(4));button(topBar,"☰  SNI TUNNEL",v->drawer());root.addView(topBar);}
        root.setOnApplyWindowInsetsListener((view,insets)->{
            int left,top,right,bottom;
            if(Build.VERSION.SDK_INT>=30){android.graphics.Insets safe=insets.getInsets(WindowInsets.Type.systemBars()|WindowInsets.Type.displayCutout()|WindowInsets.Type.ime());left=safe.left;top=safe.top;right=safe.right;bottom=safe.bottom;}
            else {left=insets.getSystemWindowInsetLeft();top=insets.getSystemWindowInsetTop();right=insets.getSystemWindowInsetRight();bottom=insets.getSystemWindowInsetBottom();DisplayCutout cutout=insets.getDisplayCutout();if(cutout!=null){left=Math.max(left,cutout.getSafeInsetLeft());top=Math.max(top,cutout.getSafeInsetTop());right=Math.max(right,cutout.getSafeInsetRight());bottom=Math.max(bottom,cutout.getSafeInsetBottom());}}
            view.setPadding(left,top,right,bottom);return insets;
        });
        LinearLayout body=new LinearLayout(this);root.addView(body,new LinearLayout.LayoutParams(-1,0,1));
        if(tv){sidebar=new LinearLayout(this);sidebar.setOrientation(LinearLayout.VERTICAL);sidebar.setPadding(dp(14),dp(28),dp(14),dp(20));sidebar.setBackgroundColor(0xff101821);ScrollView navigation=new ScrollView(this);navigation.setFillViewport(true);navigation.addView(sidebar);body.addView(navigation,new LinearLayout.LayoutParams(dp(170),-1));}
        scroll=new ScrollView(this);scroll.setFillViewport(true);body.addView(scroll,new LinearLayout.LayoutParams(0,-1,1));
        content=new LinearLayout(this);content.setOrientation(LinearLayout.VERTICAL);content.setPadding(dp(tv?28:20),dp(22),dp(tv?28:20),dp(30));scroll.addView(content);
        setContentView(root);root.requestApplyInsets();render();
        if(Build.VERSION.SDK_INT>=33&&checkSelfPermission("android.permission.POST_NOTIFICATIONS")!=android.content.pm.PackageManager.PERMISSION_GRANTED)requestPermissions(new String[]{"android.permission.POST_NOTIFICATIONS"},20);
    }
    @Override protected void onSaveInstanceState(Bundle out){out.putInt("page",page);super.onSaveInstanceState(out);}
    @Override protected void onResume(){super.onResume();handler.post(tick);}
    @Override protected void onPause(){handler.removeCallbacks(tick);super.onPause();}
    private void drawer(){
        LinearLayout menu=new LinearLayout(this);menu.setOrientation(LinearLayout.VERTICAL);menu.setPadding(dp(12),dp(22),dp(12),dp(22));menu.setBackgroundColor(0xff101821);
        PopupWindow window=new PopupWindow(menu,dp(240),-2,true);window.setBackgroundDrawable(shape(0xff101821,0xff2b3948));window.setElevation(dp(12));
        label(menu,"SNI TUNNEL",20,ACCENT);
        for(int n=0;n<PAGES.length;n++){final int dest=n;button(menu,(n==page?"●  ":"    ")+PAGES[n],v->{window.dismiss();navigate(dest);});}
        window.showAsDropDown(topBar,0,0,Gravity.START);
    }
    private void navigate(int target){page=target;render();scroll.post(()->scroll.scrollTo(0,0));}
    private void render(){
        state=null;traffic=null;activityLog=null;feedback=null;content.removeAllViews();
        if(sidebar!=null){sidebar.removeAllViews();label(sidebar,"SNI TUNNEL",19,ACCENT);label(sidebar,"ANDROID / TV",11,MUTED);for(int n=0;n<PAGES.length;n++){final int dest=n;button(sidebar,(n==page?"●  ":"")+PAGES[n],v->navigate(dest));}}
        label(content,PAGES[page],tv?28:26,INK).setTypeface(null,Typeface.BOLD);
        label(content,"Your connection. Your configuration.",14,MUTED);space(content,14);
        switch(page){case 0:overview();break;case 1:servers();break;case 2:sniProfiles();break;case 3:activity();break;case 4:preferences();break;case 5:diagnostics();break;}
        refreshStatus();
    }
    private void overview(){
        state=label(content,TunnelService.status,21,ACCENT);state.setPadding(dp(16),dp(18),dp(16),dp(18));state.setBackground(shape(0xff182b34,0xff31515b));
        label(content,"Verification before success",13,MUTED);space(content,12);
        label(content,TunnelService.active?TunnelService.serverName:selectedName(),18,INK);
        traffic=label(content,"",16,MUTED);space(content,12);
        LinearLayout actions=row(content);button(actions,"Connect",v->prepare(false));button(actions,"Disconnect",v->stop());
        LinearLayout actions2=row(content);button(actions2,"Reconnect",v->prepare(true));button(actions2,"Verify now",v->verify());
        button(content,"Switch server",v->navigate(1));
        button(content,"Choose SNI profile",v->navigate(2));
        feedback=label(content,"",14,MUTED);
        label(content,"TUN · whole device, except this app/core UID\nAndroid's VPN permission is required. Proxy HTTPS and tunneled DNS are checked independently. Transfer rates below measure packets handled by the VPN, not the proxy probe.",12,MUTED);
    }
    private void servers(){
        LinearLayout add=row(content);button(add,"+ Add server",v->edit(null));button(add,"Clipboard",v->clipboard());
        importBox=input(content,"Paste server links (one per line) or VMess JSON",true);importBox.setMinLines(2);importBox.setMaxLines(4);
        LinearLayout imports=row(content);button(imports,"Import",v->importText(importBox.getText().toString()));button(imports,"Import file",v->file());
        feedback=label(content,profiles.size()+" saved · imports create new profiles",13,ACCENT);
        search=input(content,"Search servers, address or SNI",false);search.setText(query);
        Spinner sort=spinner(content,new String[]{"Name","Latency"});sort.setSelection(sortLatency?1:0);sort.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener(){public void onItemSelected(AdapterView<?> p,View v,int n,long id){sortLatency=n==1;serverRows();}public void onNothingSelected(AdapterView<?> p){}});
        serverList=new LinearLayout(this);serverList.setOrientation(LinearLayout.VERTICAL);content.addView(serverList);
        search.addTextChangedListener(new TextWatcher(){public void beforeTextChanged(CharSequence s,int a,int c,int f){}public void onTextChanged(CharSequence s,int a,int b,int c){query=s.toString();serverRows();}public void afterTextChanged(Editable e){}});serverRows();
    }
    private void serverRows(){
        if(serverList==null)return;serverList.removeAllViews();List<Profile> shown=new ArrayList<>();
        for(Profile p:profiles)if((p.name()+" "+p.value.optString("add")+" "+p.value.optString("sni")).toLowerCase(Locale.ROOT).contains(query.toLowerCase(Locale.ROOT)))shown.add(p);
        shown.sort((a,b)->sortLatency?Long.compare(latencies.getOrDefault(a.id(),Long.MAX_VALUE),latencies.getOrDefault(b.id(),Long.MAX_VALUE)):a.name().compareToIgnoreCase(b.name()));
        for(Profile p:shown){
            LinearLayout card=card(serverList);label(card,(p.id().equals(store.selected())?"●  ":"")+p.name(),17,INK);
            label(card,p.value.optString("add")+":"+p.value.optInt("port")+"  ·  "+p.value.optString("net","tcp")+" / "+p.value.optString("tls","none"),12,MUTED);
            label(card,"SNI: "+p.value.optString("sni","default")+(latencies.containsKey(p.id())?"  ·  TCP "+latencies.get(p.id())+" ms":""),12,MUTED);
            LinearLayout a=row(card);button(a,"Use",v->{store.select(p.id());serverRows();message("Server selected. Connect or Reconnect from Overview.");});button(a,"Edit",v->edit(p));button(a,"Test",v->test(p));
            LinearLayout b=row(card);button(b,"Duplicate",v->duplicate(p));button(b,"Export",v->export(p));button(b,"Delete",v->delete(p));
        }
        if(shown.isEmpty())label(serverList,profiles.isEmpty()?"No servers yet. Add or import one above.":"No matching servers.",15,MUTED);
    }
    private void edit(Profile existing){
        if(TunnelService.active){message("Disconnect before editing a server.");return;}
        ScrollView scroll=new ScrollView(this);LinearLayout form=new LinearLayout(this);form.setOrientation(LinearLayout.VERTICAL);form.setPadding(dp(16),dp(10),dp(16),dp(10));scroll.addView(form);
        JSONObject old=existing==null?new JSONObject():existing.value;
        EditText name=field(form,"Name",old.optString("ps"));EditText address=field(form,"Dial address",old.optString("add"));EditText port=field(form,"Port",old.optString("port","443"));port.setInputType(2);
        EditText uuid=field(form,"VMess UUID",old.optString("id"));uuid.setInputType(129);
        Spinner net=spinner(form,new String[]{"tcp","ws","grpc"});net.setSelection(Arrays.asList("tcp","ws","grpc").indexOf(old.optString("net","tcp")));
        CheckBox tls=check(form,"TLS",old.optString("tls").equals("tls"));
        CheckBox insecure=check(form,"Allow insecure",old.optBoolean("allowInsecure",false));
        label(form,"Skip certificate identity verification for this server. Use only when required by your server.",12,MUTED);
        insecure.setEnabled(tls.isChecked());tls.setOnCheckedChangeListener((view,enabled)->insecure.setEnabled(enabled));
        EditText sni=field(form,"TLS SNI",old.optString("sni"));EditText host=field(form,"WebSocket Host",old.optString("host"));EditText path=field(form,"WebSocket path / gRPC service",old.optString("path","/"));
        EditText verifyName=field(form,"Certificate verification name (optional)",old.optString("verifyPeerCertByName"));
        label(form,"Verification name may differ from SNI. Applies when Allow insecure is off and no certificate pin is set.",12,MUTED);
        EditText pin=field(form,"Trusted certificate SHA256 pin (optional)",old.optString("pinnedPeerCertSha256"));
        label(form,"A leaf certificate pin replaces normal name/CA validation. Enter only a fingerprint you trust.",12,MUTED);
        EditText alpn=field(form,"ALPN (comma separated, optional)",old.optString("alpn"));
        EditText fingerprint=field(form,"TLS fingerprint (optional)",old.optString("fp"));
        Spinner encryption=spinner(form,new String[]{"auto","aes-128-gcm","chacha20-poly1305","none","zero"});encryption.setSelection(Math.max(0,Arrays.asList("auto","aes-128-gcm","chacha20-poly1305","none","zero").indexOf(old.optString("scy","auto"))));
        CheckBox mux=check(form,"Multiplexing",old.optBoolean("mux",false));
        AlertDialog dialog=new AlertDialog.Builder(this).setTitle(existing==null?"Add server":"Edit server").setView(scroll).setNegativeButton("Cancel",null).setPositiveButton("Save",null).create();dialog.show();
        dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(v->{try{
            JSONObject data=new JSONObject(old.toString()).put("ps",name.getText().toString().trim()).put("add",address.getText().toString().trim()).put("port",Integer.parseInt(port.getText().toString())).put("id",uuid.getText().toString().trim()).put("net",net.getSelectedItem().toString()).put("tls",tls.isChecked()?"tls":"").put("sni",sni.getText().toString().trim()).put("host",host.getText().toString().trim()).put("path",path.getText().toString()).put("verifyPeerCertByName",verifyName.getText().toString().trim()).put("pinnedPeerCertSha256",pin.getText().toString().trim()).put("alpn",alpn.getText().toString().trim()).put("fp",fingerprint.getText().toString().trim()).put("scy",encryption.getSelectedItem().toString()).put("mux",mux.isChecked()).put("allowInsecure",insecure.isChecked());
            Profile changed=new Profile(data);changed.validateForSave();List<Profile> candidate=new ArrayList<>();for(Profile p:profiles)candidate.add(existing!=null&&p.id().equals(existing.id())?changed:p);if(existing==null)candidate.add(changed);
            store.save(candidate);if(profiles.isEmpty())store.select(changed.id());load();dialog.dismiss();render();message("Server saved.");
        }catch(Exception e){Toast.makeText(this,Events.clean(e.getMessage()),Toast.LENGTH_LONG).show();}});
    }
    private void sniProfiles(){
        label(content,"SNI profiles override TLS SNI independently of the server address and WebSocket Host.",14,MUTED);
        button(content,"Use server's original SNI",v->{settings.prefs.edit().remove("selectedSni").apply();render();});
        button(content,"+ Add SNI profile",v->editSni(null));
        JSONArray list=settings.sniProfiles();for(int n=0;n<list.length();n++){JSONObject p=list.optJSONObject(n);if(p==null)continue;
            LinearLayout card=card(content);label(card,(p.optString("id").equals(settings.prefs.getString("selectedSni",""))?"●  ":"")+p.optString("name"),17,INK);label(card,p.optString("sni"),14,MUTED);
            LinearLayout actions=row(card);button(actions,"Use",v->{settings.prefs.edit().putString("selectedSni",p.optString("id")).apply();render();});button(actions,"Edit",v->editSni(p));button(actions,"Delete",v->{try{JSONArray next=new JSONArray();for(int j=0;j<list.length();j++)if(!list.getJSONObject(j).optString("id").equals(p.optString("id")))next.put(list.getJSONObject(j));settings.saveSni(next);render();}catch(Exception e){message("Could not save SNI profiles.");}});
        }
        feedback=label(content,"SNI changes apply on the next Connect or Reconnect.",13,MUTED);
    }
    private void editSni(JSONObject previous){
        LinearLayout form=new LinearLayout(this);form.setPadding(dp(18),dp(8),dp(18),dp(8));form.setOrientation(LinearLayout.VERTICAL);
        EditText name=field(form,"Name",previous==null?"":previous.optString("name"));EditText host=field(form,"TLS SNI",previous==null?"":previous.optString("sni"));
        AlertDialog dialog=new AlertDialog.Builder(this).setTitle("SNI profile").setView(form).setNegativeButton("Cancel",null).setPositiveButton("Save",null).create();dialog.show();dialog.getButton(-1).setOnClickListener(v->{try{
            String sni=host.getText().toString().trim();if(sni.isEmpty()||sni.contains("/")||sni.matches(".*\\s.*"))throw new Exception("Enter a valid SNI hostname");
            JSONObject changed=new JSONObject().put("id",previous==null?UUID.randomUUID().toString():previous.getString("id")).put("name",name.getText().toString().trim()).put("sni",sni);JSONArray next=new JSONArray();JSONArray old=settings.sniProfiles();for(int n=0;n<old.length();n++){JSONObject p=old.getJSONObject(n);next.put(previous!=null&&p.getString("id").equals(previous.getString("id"))?changed:p);}if(previous==null)next.put(changed);settings.saveSni(next);dialog.dismiss();render();
        }catch(Exception e){Toast.makeText(this,Events.clean(e.getMessage()),Toast.LENGTH_LONG).show();}});
    }
    private void activity(){
        LinearLayout actions=row(content);button(actions,"Copy logs",v->{getSystemService(ClipboardManager.class).setPrimaryClip(ClipData.newPlainText("SNI Tunnel events",Events.text()));message("Redacted events copied.");});button(actions,"Clear",v->{Events.clear();refreshStatus();});
        activityLog=label(content,Events.text(),12,MUTED);activityLog.setTypeface(Typeface.MONOSPACE);activityLog.setTextIsSelectable(true);feedback=label(content,"UUIDs and server links are hidden from these events.",12,MUTED);
    }
    private void preferences(){
        CheckBox health=check(content,"Automatic health checks",settings.health());CheckBox recovery=check(content,"Automatic reconnect (bounded attempts)",settings.reconnect());
        EditText interval=field(content,"Health interval (15–3600 seconds)",String.valueOf(settings.interval()));interval.setInputType(2);
        EditText timeout=field(content,"Probe timeout (3–30 seconds)",String.valueOf(settings.timeout()));timeout.setInputType(2);
        button(content,"Save settings",v->{try{int seconds=Integer.parseInt(interval.getText().toString()),limit=Integer.parseInt(timeout.getText().toString());if(seconds<15||seconds>3600||limit<3||limit>30)throw new Exception();settings.prefs.edit().putBoolean("health",health.isChecked()).putBoolean("reconnect",recovery.isChecked()).putInt("interval",seconds).putInt("timeout",limit).apply();message("Settings saved.");}catch(Exception e){message("Enter interval 15–3600 and timeout 3–30.");}});
        label(content,"Version " + BuildConfig.VERSION_NAME,12,MUTED);
        label(content,"TUN routes the whole device except this app/core UID. Android's VPN dialog controls permission. A failed startup leaves ordinary connectivity available; bounded recovery retains an established VPN. Android always-on/lockdown is not provided.",13,MUTED);
        feedback=label(content,"",14,ACCENT);
    }
    private void diagnostics(){
        state=label(content,TunnelService.status,18,ACCENT);traffic=label(content,"",14,MUTED);
        button(content,"Test selected server TCP",v->{try{test(settings.apply(store.current()));}catch(Exception e){message("Select a server first.");}});
        button(content,"Verify active tunnel",v->verify());button(content,"Reconnect now",v->prepare(true));button(content,"Open Activity",v->navigate(3));
        label(content,"If TLS fails with a custom SNI, check the certificate verification name or a trusted leaf certificate pin in Servers → Edit. Proxy verification requires HTTPS plus a tunneled DNS answer. It does not certify every app route. Activity separates TLS, DNS and packet-forwarder failures.",13,MUTED);
        feedback=label(content,"",14,MUTED);
    }
    private void refreshStatus(){
        if(state!=null){state.setText(TunnelService.status);state.setTextColor(TunnelService.verified?ACCENT:TunnelService.active?0xfff3c675:MUTED);}
        if(traffic!=null){long elapsed=TunnelService.connectedAt==0?0:(System.currentTimeMillis()-TunnelService.connectedAt)/1000;traffic.setText("HTTPS latency: "+(TunnelService.latency<0?"not measured":TunnelService.latency+" ms")+"\nDownload: "+bytes(TunnelService.downloadRate)+"/s  ·  Upload: "+bytes(TunnelService.uploadRate)+"/s\nReceived: "+bytes(TunnelService.downloaded)+"  ·  Sent: "+bytes(TunnelService.uploaded)+"\nSession: "+String.format(Locale.US,"%02d:%02d",elapsed/60,elapsed%60));}
        if(activityLog!=null){String value=Events.text();if(!activityLog.getText().toString().equals(value))activityLog.setText(value);}
    }
    private void prepare(boolean reconnect){if(TunnelService.active){if(reconnect)startService(new Intent(this,TunnelService.class).setAction("RECONNECT"));else message("Connection already active. Use Reconnect or Disconnect.");return;}try{settings.apply(store.current());}catch(Exception e){message("Import and select a server first.");return;}Intent permission=VpnService.prepare(this);if(permission!=null)startActivityForResult(permission,11);else connect();}
    private void connect(){startForegroundService(new Intent(this,TunnelService.class).setAction("CONNECT"));}
    private void stop(){if(TunnelService.active)startService(new Intent(this,TunnelService.class).setAction("STOP"));}
    private void verify(){if(!TunnelService.active){message("Connect first.");return;}TunnelService.verifyRequested=true;message("Traffic verification requested.");}
    private void load(){try{profiles=store.load();readable=true;}catch(Exception e){readable=false;Events.add("Encrypted profile load failed; existing data retained");}}
    private String selectedName(){try{return store.current().name();}catch(Exception e){return "No server selected";}}
    private void importText(String raw){if(!readable){message("Saved profiles could not be decrypted. Existing data was retained.");return;}try{List<Profile> added=Profile.parse(raw),candidate=new ArrayList<>(profiles);candidate.addAll(added);store.save(candidate);if(profiles.isEmpty())store.select(added.get(0).id());load();query="";render();message("Added "+added.size()+" server(s); "+candidate.size()+" saved. Existing profiles kept.");}catch(Exception e){message("Import failed: "+Events.clean(e.getMessage())+". Nothing was replaced.");}}
    private void clipboard(){ClipboardManager c=getSystemService(ClipboardManager.class);if(!c.hasPrimaryClip()||c.getPrimaryClip()==null){message("Clipboard is empty.");return;}CharSequence text=c.getPrimaryClip().getItemAt(0).coerceToText(this);if(text!=null)importText(text.toString());}
    private void file(){try{startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT).setType("*/*").addCategory(Intent.CATEGORY_OPENABLE),12);}catch(Exception e){message("No file picker is installed. Paste a link instead.");}}
    private void duplicate(Profile source){try{Profile p=new Profile(source.value);p.value.put("localId",UUID.randomUUID().toString()).put("ps",source.name()+" (copy)");List<Profile> candidate=new ArrayList<>(profiles);candidate.add(p);store.save(candidate);load();render();message("Profile duplicated.");}catch(Exception e){message("Could not duplicate profile.");}}
    private void export(Profile p){new AlertDialog.Builder(this).setTitle("Copy server credentials?").setMessage("The VMess link includes account credentials. Keep it private.").setNegativeButton("Cancel",null).setPositiveButton("Copy",(d,w)->{String link="vmess://"+java.util.Base64.getEncoder().encodeToString(p.value.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));getSystemService(ClipboardManager.class).setPrimaryClip(ClipData.newPlainText("VMess server",link));message("Server link copied.");}).show();}
    private void delete(Profile p){if(TunnelService.active){message("Disconnect before deleting a server.");return;}new AlertDialog.Builder(this).setTitle("Delete "+p.name()+"?").setNegativeButton("Cancel",null).setPositiveButton("Delete",(d,w)->{try{List<Profile> candidate=new ArrayList<>();for(Profile old:profiles)if(!old.id().equals(p.id()))candidate.add(old);store.save(candidate);if(p.id().equals(store.selected()))store.select(candidate.isEmpty()?"":candidate.get(0).id());load();render();}catch(Exception e){message("Could not delete server. Saved data was retained.");}}).show();}
    private void test(Profile p){message("Testing TCP…");new Thread(()->{long began=System.nanoTime();try(Socket socket=new Socket()){socket.connect(new InetSocketAddress(p.value.getString("add"),p.value.getInt("port")),settings.timeout()*1000);long ms=(System.nanoTime()-began)/1000000;handler.post(()->{latencies.put(p.id(),ms);if(page==1)serverRows();message("TCP "+ms+" ms. This does not verify VMess/TLS forwarding.");});}catch(Exception e){handler.post(()->message("TCP failed: "+Events.clean(e.getMessage())));}},"server-test").start();}
    @Override protected void onActivityResult(int request,int result,Intent data){super.onActivityResult(request,result,data);if(request==11){if(result==RESULT_OK)connect();else message("VPN permission was not granted.");}if(request==12&&result==RESULT_OK&&data!=null&&data.getData()!=null){final android.net.Uri uri=data.getData();new Thread(()->{try(InputStream in=getContentResolver().openInputStream(uri);ByteArrayOutputStream out=new ByteArrayOutputStream()){if(in==null)throw new Exception();byte[] buffer=new byte[4096];int n;while((n=in.read(buffer))!=-1){if(out.size()+n>1024*1024)throw new Exception();out.write(buffer,0,n);}String raw=new String(out.toByteArray(),java.nio.charset.StandardCharsets.UTF_8);handler.post(()->importText(raw));}catch(Exception e){handler.post(()->message("Could not read UTF-8 file smaller than 1 MB."));}},"profile-import").start();}}
    private void message(String value){if(feedback!=null)feedback.setText(value);else Toast.makeText(this,value,Toast.LENGTH_LONG).show();}
    private int dp(int value){return Math.round(value*getResources().getDisplayMetrics().density);}
    private TextView label(LinearLayout parent,String value,int size,int color){TextView v=new TextView(this);v.setText(value);v.setTextSize(size);v.setTextColor(color);v.setPadding(0,dp(5),0,dp(5));parent.addView(v);return v;}
    private EditText field(LinearLayout parent,String title,String value){label(parent,title,12,MUTED);EditText v=input(parent,title,false);v.setText(value);return v;}
    private EditText input(LinearLayout parent,String hint,boolean multi){EditText v=new EditText(this);v.setHint(hint);v.setTextColor(INK);v.setHintTextColor(MUTED);v.setTextSize(14);v.setPadding(dp(12),dp(12),dp(12),dp(12));v.setBackground(focus());v.setSingleLine(!multi);v.setInputType(android.text.InputType.TYPE_CLASS_TEXT|android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS|(multi?android.text.InputType.TYPE_TEXT_FLAG_MULTI_LINE:0));LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(-1,-2);p.setMargins(0,dp(5),0,dp(8));parent.addView(v,p);return v;}
    private CheckBox check(LinearLayout parent,String title,boolean value){CheckBox v=new CheckBox(this);v.setText(title);v.setTextColor(INK);v.setChecked(value);v.setMinHeight(dp(48));parent.addView(v);return v;}
    private Spinner spinner(LinearLayout parent,String[] values){Spinner v=new Spinner(this);ArrayAdapter<String> a=new ArrayAdapter<>(this,android.R.layout.simple_spinner_item,values);a.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);v.setAdapter(a);v.setMinimumHeight(dp(48));parent.addView(v);return v;}
    private LinearLayout row(LinearLayout parent){LinearLayout v=new LinearLayout(this);parent.addView(v);return v;}
    private LinearLayout card(LinearLayout parent){LinearLayout v=new LinearLayout(this);v.setOrientation(LinearLayout.VERTICAL);v.setPadding(dp(12),dp(10),dp(12),dp(10));v.setBackground(shape(0xff16212d,0xff253647));LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(-1,-2);p.setMargins(0,dp(6),0,dp(8));parent.addView(v,p);return v;}
    private void button(LinearLayout parent,String text,View.OnClickListener action){Button v=new Button(this);v.setText(text);v.setTextSize(tv?15:13);v.setAllCaps(false);v.setTextColor(INK);v.setBackground(focus());v.setMinHeight(dp(48));v.setFocusable(true);v.setOnClickListener(action);boolean vertical=parent.getOrientation()==LinearLayout.VERTICAL;LinearLayout.LayoutParams p=new LinearLayout.LayoutParams(vertical?-1:0,-2,vertical?0:1);p.setMargins(dp(3),dp(5),dp(3),dp(5));parent.addView(v,p);}
    private GradientDrawable shape(int color,int border){GradientDrawable d=new GradientDrawable();d.setColor(color);d.setCornerRadius(dp(10));d.setStroke(dp(2),border);return d;}
    private StateListDrawable focus(){StateListDrawable d=new StateListDrawable();d.addState(new int[]{android.R.attr.state_focused},shape(0xff244b58,ACCENT));d.addState(new int[]{android.R.attr.state_pressed},shape(0xff244b58,ACCENT));d.addState(new int[]{},shape(0xff18232f,0xff2b3948));return d;}
    private void space(LinearLayout parent,int height){parent.addView(new View(this),new LinearLayout.LayoutParams(1,dp(height)));}
    private static String bytes(long n){return n<1024?n+" B":n<1048576?String.format(Locale.US,"%.1f KB",n/1024.0):String.format(Locale.US,"%.1f MB",n/1048576.0);}
}
