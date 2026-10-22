"""Exercise the distributable installer without root or a real install."""
import subprocess
import tempfile
import unittest
from pathlib import Path

class BundleTests(unittest.TestCase):
    def test_checksums_extract_spaces_and_reject_tampering(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            (base/'bundle').mkdir()
            (base/'bundle/payload').write_text('synthetic test payload')
            subprocess.run(['python3', str(Path(__file__).with_name('finish.py')), str(base/'bundle'), 'arch', str(base/'out')], check=True, stdout=subprocess.DEVNULL)
            installer = next((base/'out').glob('*.bin'))
            subprocess.run(['sh', str(installer), '--check'], check=True, stdout=subprocess.DEVNULL)
            extracted = base/'directory with spaces'
            subprocess.run(['sh', str(installer), '--extract', str(extracted)], check=True, stdout=subprocess.DEVNULL)
            subprocess.run(['bash', str(extracted/'install.sh'), '--check'], check=True, stdout=subprocess.DEVNULL)
            (extracted/'payload').write_text('changed')
            self.assertNotEqual(subprocess.run(['bash', str(extracted/'install.sh'), '--check'], capture_output=True).returncode, 0)
            with installer.open('ab') as stream:
                stream.write(b'changed')
            self.assertNotEqual(subprocess.run(['sh', str(installer), '--check'], capture_output=True).returncode, 0)

if __name__ == '__main__':
    unittest.main()
