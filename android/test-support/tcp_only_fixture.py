from pathlib import Path
import socket,socketserver,struct,json,threading,http.server,subprocess,hashlib
root=Path.cwd();import argparse
parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,default=root/'work/android-dns-fixture');args=parser.parse_args();work=args.output.resolve();work.mkdir(parents=True,exist_ok=True)
payload=bytes(range(256))*8192
class HTTP(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  self.send_response(200);self.send_header('Content-Length',str(len(payload)));self.end_headers();self.wfile.write(payload)
 def log_message(self,*a):pass
http=http.server.ThreadingHTTPServer(('127.0.0.1',0),HTTP);threading.Thread(target=http.serve_forever,daemon=True).start()
def read(s,n):
 b=b''
 while len(b)<n:
  q=s.recv(n-len(b))
  if not q:raise EOFError()
  b+=q
 return b
class DNS(socketserver.BaseRequestHandler):
 def handle(self):
  try:
   self.request.settimeout(15)
   while True:
    q=read(self.request,struct.unpack('!H',read(self.request,2))[0]);p=12
    while q[p]:p+=1+q[p]
    p+=1;kind=struct.unpack('!H',q[p:p+2])[0];question=q[12:p+4]
    answer=b'\xc0\x0c'+struct.pack('!HHIH',1,1,30,4)+socket.inet_aton('198.18.0.42') if kind==1 else b''
    response=q[:2]+struct.pack('!HHHHH',0x8180,1,int(kind==1),0,0)+question+answer
    self.request.sendall(struct.pack('!H',len(response))+response)
    with (work/'dns-requests.log').open('a') as f:f.write('TCP DNS response\n')
  except (OSError,EOFError):pass
class TCP(socketserver.ThreadingTCPServer):allow_reuse_address=True;daemon_threads=True
dns=TCP(('127.0.0.1',0),DNS);threading.Thread(target=dns.serve_forever,daemon=True).start()
with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
uuid='00000000-0000-4000-8000-000000000104'
config={'log':{'access':str(work/'access.log'),'loglevel':'warning'},'inbounds':[{'listen':'127.0.0.1','port':port,'protocol':'vmess','settings':{'clients':[{'id':uuid,'alterId':0}]}}],'outbounds':[{'protocol':'freedom','tag':'direct'},{'protocol':'blackhole','tag':'drop-udp'},{'protocol':'freedom','tag':'download','settings':{'redirect':'127.0.0.1:'+str(http.server_port)}},{'protocol':'freedom','tag':'dns-tcp','settings':{'redirect':'127.0.0.1:'+str(dns.server_address[1])}}],'routing':{'rules':[{'type':'field','network':'udp','outboundTag':'drop-udp'},{'type':'field','ip':['1.1.1.1'],'port':'53','network':'tcp','outboundTag':'dns-tcp'},{'type':'field','ip':['198.18.0.42'],'outboundTag':'download'}]}}
(work/'server.json').write_text(json.dumps(config));(work/'profile.json').write_text(json.dumps({'add':'10.0.2.2','port':port,'id':uuid,'net':'tcp','tls':'','ps':'TCP-only regression fixture'}));(work/'expected.json').write_text(json.dumps({'bytes':len(payload),'sha256':hashlib.sha256(payload).hexdigest()}))
p=subprocess.Popen([str(root/'packaging/staging/linux/core/xray'),'run','-config',str(work/'server.json')]);print('Controlled TCP-only fixture ready',flush=True)
try:p.wait()
finally:p.terminate();http.shutdown();dns.shutdown()
