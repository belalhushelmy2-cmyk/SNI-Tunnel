#!/usr/bin/env python3
"""Build signed phone/TV APKs; retain the private signing key outside source exports."""
from pathlib import Path
import argparse,os,secrets,subprocess,shutil,hashlib,json
ROOT=Path(__file__).resolve().parent
PROJECT=ROOT.parent

def main():
 parser=argparse.ArgumentParser();parser.add_argument('--gradle',default=str(ROOT/'gradlew'));parser.add_argument('--output',type=Path,default=PROJECT.parent/'outputs/android/1.0.0');args=parser.parse_args()
 subprocess.run(['python3',str(ROOT/'fetch-runtime.py')],check=True)
 env=os.environ.copy()
 if not env.get('SNI_ANDROID_KEYSTORE'):
  private=PROJECT/'private/android-signing';private.mkdir(parents=True,exist_ok=True);private.chmod(0o700)
  password=private/'password';keystore=private/'release.jks'
  if not password.exists():password.write_text(secrets.token_urlsafe(40));password.chmod(0o600)
  env['SNI_ANDROID_STORE_PASSWORD']=password.read_text().strip();env['SNI_ANDROID_KEYSTORE']=str(keystore)
  if not keystore.exists():
   subprocess.run(['keytool','-genkeypair','-keystore',str(keystore),'-storepass:env','SNI_ANDROID_STORE_PASSWORD','-keypass:env','SNI_ANDROID_STORE_PASSWORD','-alias','sni-tunnel','-keyalg','RSA','-keysize','3072','-validity','10000','-dname','CN=SNI Tunnel Android, O=SNI Tunnel'],env=env,check=True)
   keystore.chmod(0o600)
 subprocess.run([args.gradle,'-p',str(ROOT),'testPhoneDebugUnitTest','testTvDebugUnitTest','lintPhoneRelease','lintTvRelease','assemblePhoneRelease','assembleTvRelease','--console=plain'],env=env,check=True)
 args.output.mkdir(parents=True,exist_ok=True)
 hashes=[]
 for flavor in ('phone','tv'):
  source=ROOT/'app/build/outputs/apk'/flavor/'release'/('app-'+flavor+'-release.apk')
  dest=args.output/('SNI-Tunnel-'+('Android-TV' if flavor=='tv' else 'Android')+'-1.0.0.apk')
  shutil.copyfile(source,dest);hashes.append(hashlib.sha256(dest.read_bytes()).hexdigest()+'  '+dest.name)
 (args.output/'SHA256SUMS').write_text('\n'.join(hashes)+'\n')
 shutil.copyfile(ROOT/'README.md',args.output/'README.md')
 print('Signed phone and TV APKs: '+str(args.output))
 print('Retain the private signing key securely; future Android updates require the same key.')
if __name__=='__main__':main()
