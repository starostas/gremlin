#!/usr/bin/env python3
"""Local browser demo; standard library only. One bounded synthesis job at a time."""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import shlex
import signal
import subprocess
import threading
import urllib.parse

ROOT = Path(__file__).resolve().parent


def serve(args):
    lock = threading.Lock()
    state = {'running': False, 'events': [], 'process': None, 'cancelled': False}
    capabilities = {'gpu': bool(args.ssh or args.gpu),
                    'live': bool(args.ssh or (args.gpu and args.engine.is_file())),
                    'remote': bool(args.ssh)}

    def event(value):
        with lock:
            state['events'].append(dict(value, id=len(state['events']) + 1))

    def stop():
        with lock:
            state['cancelled'] = True
            process = state['process']
        if process and process.poll() is None:
            try:
                import os
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass

    def job(settings):
        process = None
        watchdog = None
        try:
            event({'kind': 'input', 'pixels': settings['target']})
            payload = json.dumps(settings, separators=(',', ':'))
            if args.ssh:
                command = ['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10',
                           '-p', str(args.ssh_port), args.ssh,
                           shlex.join(['timeout', str(420 if settings['budget_ms'] > 12000 else 180), args.remote_engine])]
            else:
                command = [str(args.engine.resolve())]
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                       text=True, stdin=subprocess.PIPE, start_new_session=True)
            with lock:
                state['process'] = process
                cancelled = state['cancelled']
            if cancelled:
                stop()
            watchdog = threading.Timer(440 if settings['budget_ms'] > 12000 else 200, stop)
            watchdog.start()
            process.stdin.write(payload)
            process.stdin.close()
            completed = False
            terminal = False
            for line in process.stdout:
                if len(line) > 24_000_000:
                    raise RuntimeError('Engine event exceeds limit')
                try:
                    value = json.loads(line)
                except json.JSONDecodeError:
                    if line.strip():
                        event({'kind': 'diagnostic', 'message': line.strip()[:500]})
                    continue
                if not isinstance(value, dict):
                    raise RuntimeError('Invalid engine event')
                event(value)
                completed = value.get('kind') == 'done' and value.get('mode') == 'gpu'
                terminal = completed or value.get('kind') == 'error'
                if terminal:
                    break
            try:
                code = process.wait(timeout=.2 if terminal else None)
            except subprocess.TimeoutExpired:
                # Some SSH gateways keep the channel open after the engine exits.
                # Its explicit final event establishes completion; close our own
                # transport process so the next bounded job can start.
                import os
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=5)
                code = 0 if completed else 1
            with lock:
                cancelled = state['cancelled']
            if code:
                event({'kind': 'error', 'message': 'Run stopped or timed out.' if cancelled
                       else f'Engine exited with code {code}; inspect the diagnostic above.'})
            event({'kind': 'finished', 'exit_code': code})
        except Exception as error:
            stop()
            event({'kind': 'error', 'message': str(error)})
            event({'kind': 'finished', 'exit_code': 1})
        finally:
            if watchdog:
                watchdog.cancel()
            if process:
                if process.poll() is None:
                    process.terminate()
                process.wait()
            with lock:
                state['running'] = False
                state['process'] = None

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def send(self, status, value, mime='application/json'):
            data = json.dumps(value).encode() if mime == 'application/json' else value
            self.send_response(status)
            self.send_header('Content-Type', mime)
            self.send_header('Content-Length', str(len(data)))
            self.send_header('Cache-Control', 'no-store')
            self.send_header('X-Content-Type-Options', 'nosniff')
            self.end_headers()
            self.wfile.write(data)

        def allowed_host(self):
            host = self.headers.get('Host')
            if args.host == '0.0.0.0':
                return bool(host)
            return host in [f'{args.host}:{args.port}', f'127.0.0.1:{args.port}', f'localhost:{args.port}']

        def do_GET(self):
            if not self.allowed_host():
                return self.send(403, {'error': 'Request host or origin not allowed'})
            url = urllib.parse.urlparse(self.path)
            if url.path == '/api/state':
                with lock:
                    value = {'running': state['running'], 'capabilities': capabilities}
                return self.send(200, value)
            if url.path == '/api/events':
                try:
                    after = int(urllib.parse.parse_qs(url.query).get('after', ['0'])[0])
                    if after < 0:
                        raise ValueError()
                except ValueError:
                    return self.send(400, {'error': 'Invalid event cursor'})
                with lock:
                    value = {'events': state['events'][after:], 'running': state['running']}
                return self.send(200, value)
            files = {'/': ('index.html', 'text/html; charset=utf-8'),
                     '/app.js': ('app.js', 'text/javascript; charset=utf-8'),
                     '/graphics.js': ('graphics.js', 'text/javascript; charset=utf-8'),
                     '/style.css': ('style.css', 'text/css; charset=utf-8'),
                     '/sample.json': ('sample.json', 'application/octet-stream')}
            if url.path not in files:
                return self.send(404, {'error': 'Not found'})
            name, mime = files[url.path]
            try:
                self.send(200, (ROOT / name).read_bytes(), mime)
            except FileNotFoundError:
                self.send(404, {'error': 'Recording not available'})

        def do_POST(self):
            origin = self.headers.get('Origin')
            if not self.allowed_host() or (origin and origin != 'http://' + self.headers['Host']):
                return self.send(403, {'error': 'Request host or origin not allowed'})
            if self.headers.get('Content-Type') != 'application/json':
                return self.send(415, {'error': 'JSON required'})
            try:
                length = int(self.headers.get('Content-Length', '0'))
                if not 0 < length <= 50_000_000:
                    raise ValueError('Invalid request size')
                settings = json.loads(self.rfile.read(length))
                if self.path == '/api/cancel':
                    stop()
                    return self.send(200, {'stopping': True})
                if self.path != '/api/run':
                    return self.send(404, {'error': 'Not found'})
                if (not isinstance(settings, dict)
                        or set(settings) != {'target', 'seed', 'budget_ms'}
                        or type(settings['seed']) is not int or settings['seed'] not in (1, 2, 3)
                        or type(settings['budget_ms']) is not int or settings['budget_ms'] not in (1500, 3000, 12000, 24000, 48000)
                        or not isinstance(settings['target'], list) or len(settings['target']) not in tuple(w*w for w in (128, 256, 512, 1024, 2048))
                        or any(type(p) is not int or not 0 <= p <= 0xffffff for p in settings['target'])):
                    raise ValueError('Expected square RGB pixels at a supported resolution, seed 1–3, and a supported budget')
                if not capabilities['live']:
                    return self.send(400, {'error': 'GPU engine is not configured. Replay is available.'})
                with lock:
                    if state['running']:
                        return self.send(409, {'error': 'A run is already active'})
                    state.update(running=True, events=[], cancelled=False)
                threading.Thread(target=job, args=(settings,), daemon=True).start()
                self.send(202, {'started': True})
            except (ValueError, TypeError) as error:
                self.send(400, {'error': str(error)})

    server = ThreadingHTTPServer((args.host, args.port), Handler)
    print(f'Shader Sculptor: http://{args.host}:{args.port}', flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        stop()
        server.server_close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', default='127.0.0.1', help='Bind address; use 0.0.0.0 for network access')
    parser.add_argument('--port', type=int, default=8790)
    parser.add_argument('--engine', type=Path, default=ROOT / 'engine/target/release/shader-sculptor')
    parser.add_argument('--gpu', action='store_true', help='Local engine was built with CUDA')
    parser.add_argument('--ssh', help='Run the engine on user@host via existing SSH credentials')
    parser.add_argument('--ssh-port', type=int, default=22)
    parser.add_argument('--remote-engine', default='/root/gremlin-validation/apps/shader-sculptor/engine/target/release/shader-sculptor')
    serve(parser.parse_args())
