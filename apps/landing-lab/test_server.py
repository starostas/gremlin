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
        with tempfile.TemporaryDirectory(prefix='landing-server-test-') as temp:
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
                self.assertTrue(all(e['safe'] == e['cases'] for e in sample['events'] if e['kind'] == 'done'))
                valid = {'mode': 'gpu', 'preset': 'lander', 'cases': 8192, 'seed': 1}
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

    def test_final_event_releases_a_lingering_process(self):
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        with tempfile.TemporaryDirectory(prefix='landing-final-test-') as temp:
            engine = Path(temp) / 'fake-engine'
            engine.write_text('#!/usr/bin/env python3\nimport time\nprint(\'{"kind":"done","mode":"cpu"}\', flush=True)\ntime.sleep(60)\n')
            engine.chmod(0o700)
            process = subprocess.Popen([sys.executable, str(ROOT / 'server.py'), '--port', str(port),
                                        '--engine', str(engine)], stdout=subprocess.DEVNULL,
                                       stderr=subprocess.PIPE)
            base = f'http://127.0.0.1:{port}'
            try:
                for _ in range(100):
                    try:
                        with urllib.request.urlopen(base + '/api/state', timeout=1):
                            break
                    except urllib.error.URLError:
                        time.sleep(.03)
                settings = {'mode': 'cpu', 'preset': 'lander', 'cases': 2048, 'seed': 1}
                request = urllib.request.Request(base + '/api/run', data=json.dumps(settings).encode(),
                                                 headers={'Content-Type': 'application/json'})
                with urllib.request.urlopen(request) as response:
                    self.assertEqual(response.status, 202)
                for _ in range(100):
                    with urllib.request.urlopen(base + '/api/events') as response:
                        data = json.load(response)
                    if not data['running']:
                        break
                    time.sleep(.05)
                self.assertFalse(data['running'])
                self.assertEqual(data['events'][-1]['kind'], 'finished')
                self.assertEqual(data['events'][-1]['exit_code'], 0)
            finally:
                process.terminate()
                process.wait(timeout=5)
                process.stderr.close()


if __name__ == '__main__':
    unittest.main()
