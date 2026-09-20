"""HTTP validation and replay-only startup, using only the Python standard library."""
import json
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import unittest
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parent


class ServerTest(unittest.TestCase):
    def test_replay_only_and_request_boundaries(self):
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        with tempfile.TemporaryDirectory(prefix='shader-server-test-') as temp:
            process = subprocess.Popen([sys.executable, str(ROOT / 'server.py'), '--port', str(port),
                                        '--engine', str(Path(temp) / 'missing-engine')],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
            base = f'http://127.0.0.1:{port}'
            try:
                for _ in range(100):
                    try:
                        with urllib.request.urlopen(base + '/api/state', timeout=1) as response:
                            state = json.load(response)
                        break
                    except urllib.error.URLError:
                        time.sleep(.03)
                else:
                    self.fail('Server did not start')
                self.assertFalse(state['capabilities']['live'])
                self.assertFalse(state['capabilities']['gpu'])
                with urllib.request.urlopen(base + '/sample.json') as response:
                    sample = json.load(response)
                self.assertTrue(sample['recorded'])
                self.assertTrue(all(e['success'] for e in sample['events'] if e['kind'] == 'done'))
                valid = {'mode': 'gpu', 'preset': 'afterglow', 'cases': 8192, 'seed': 1}
                for settings in [valid, dict(valid, cases=10**9), dict(valid, seed=True), [], {}]:
                    request = urllib.request.Request(base + '/api/run', data=json.dumps(settings).encode(),
                                                     headers={'Content-Type': 'application/json'})
                    with self.assertRaises(urllib.error.HTTPError) as error:
                        urllib.request.urlopen(request)
                    self.assertEqual(error.exception.code, 400)
                foreign = urllib.request.Request(base + '/api/cancel', data=b'{}',
                                                  headers={'Content-Type': 'application/json', 'Origin': 'http://example.invalid'})
                with self.assertRaises(urllib.error.HTTPError) as error:
                    urllib.request.urlopen(foreign)
                self.assertEqual(error.exception.code, 403)
                with self.assertRaises(urllib.error.HTTPError) as error:
                    urllib.request.urlopen(base + '/engine/Cargo.toml')
                self.assertEqual(error.exception.code, 404)
            finally:
                process.terminate()
                process.wait(timeout=5)
                process.stderr.close()


if __name__ == '__main__':
    unittest.main()
