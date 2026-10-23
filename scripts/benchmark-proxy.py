#!/usr/bin/env python3
"""Compare the real checkpoint relay with current modes, using loopback only.

No user store, real VMess account, system proxy, TUN or external traffic is used.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-ref', default='ef7a36e344d2be62b48f52c4ced710df3a1eeccb')
    parser.add_argument('--output', type=Path, default=ROOT/'work/performance/reproducible')
    parser.add_argument('--rounds', type=int, default=2, choices=range(1, 6))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    before = subprocess.check_output(['git', 'show', f'{args.baseline_ref}:src/localproxy.rs'], cwd=ROOT, text=True).lstrip('\ufeff')
    harness = (ROOT/'examples/perf_probe.rs').read_text()
    harness = harness.replace('localproxy::LocalProxy, ', '')
    # Adapt only the new constructor signature; checkpoint behavior is intact.
    adapter = '''
impl baseline::LocalProxy {
    async fn start_with_mode(http:u16,socks:u16,upstream:std::net::SocketAddr,
        traffic:std::sync::Arc<stats::Traffic>,logger:logging::Logger,
        _mode:sni_tunnel::performance::PerformanceMode)->anyhow::Result<Self> {
        Self::start(http,socks,upstream,traffic,logger).await
    }
}
'''
    generated = ROOT/'examples/__perf_baseline_generated.rs'
    if generated.exists():
        raise SystemExit(f'Refusing to overwrite {generated}')
    # Inner doc comments must stay at the beginning of their own module.
    source = 'use sni_tunnel::{logging, stats};\nuse baseline::LocalProxy;\nmod baseline {\n'+before+'\n}\n'+adapter+harness.replace('//!', '//')
    generated.write_text(source)
    metadata = {'scope':'loopback only; proxy plus synthetic SOCKS core; no VMess/TUN/Internet throughput claim',
                'baseline_ref':args.baseline_ref,
                'baseline_blob':subprocess.check_output(['git','rev-parse',f'{args.baseline_ref}:src/localproxy.rs'],cwd=ROOT,text=True).strip(),
                'results':[]}
    try:
        with (args.output/'build.log').open('w') as log:
            subprocess.run(['cargo','build','--release','--locked','--example','perf_probe','--example','__perf_baseline_generated'],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,check=True)
        suffix='.exe' if os.name=='nt' else ''
        variants=[('before','__perf_baseline_generated',[]),('balanced','perf_probe',[]),('low_latency','perf_probe',['low_latency']),('maximum_throughput','perf_probe',['maximum_throughput'])]
        for round_number in range(args.rounds):
            for name,binary,options in variants if round_number%2==0 else reversed(variants):
                output=args.output/f'{round_number}-{name}.json'
                started=time.monotonic()
                with output.open('w') as out, (args.output/f'{round_number}-{name}.log').open('w') as err:
                    process=subprocess.Popen([str(ROOT/'target/release/examples'/(binary+suffix)),*options],cwd=ROOT,stdout=out,stderr=err)
                    if hasattr(os,'wait4'):
                        _,status,usage=os.wait4(process.pid,0)
                        process.returncode=os.waitstatus_to_exitcode(status)
                        resources={'user_cpu_s':usage.ru_utime,'system_cpu_s':usage.ru_stime,'peak_rss_kib':usage.ru_maxrss}
                    else:
                        process.wait();resources={}
                    if process.returncode:
                        raise RuntimeError(f'Benchmark failed: {name}; see {err.name}')
                result=json.loads(output.read_text())
                metadata['results'].append({'round':round_number,'variant':name,'wall_s':time.monotonic()-started,'resources':resources,'measurements':result})
                (args.output/'comparison.json').write_text(json.dumps(metadata,indent=2)+'\n')
                print(f'Round {round_number+1}: {name} passed',flush=True)
    finally:
        generated.unlink(missing_ok=True)


if __name__ == '__main__':
    main()
