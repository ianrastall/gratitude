"""Exercise bundled aria2 against a tiny, locally generated torrent/webseed."""
from pathlib import Path
import hashlib
import http.server
import functools
import subprocess
import threading
import tempfile

ROOT = Path(__file__).resolve().parents[1]

def bencode(value):
    if isinstance(value, int): return b'i'+str(value).encode()+b'e'
    if isinstance(value, bytes): return str(len(value)).encode()+b':'+value
    if isinstance(value, list): return b'l'+b''.join(map(bencode,value))+b'e'
    if isinstance(value, dict): return b'd'+b''.join(bencode(k)+bencode(value[k]) for k in sorted(value))+b'e'
    raise TypeError(value)

class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args): pass

def main():
    work=ROOT/'work'
    work.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='torrent-check-',dir=work) as temporary:
        temporary=Path(temporary)
        original=(b'Jupiter Show Player webseed test\n'*2048)
        (temporary/'fixture.bin').write_bytes(original)
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(QuietHandler,directory=str(temporary)))
        threading.Thread(target=server.serve_forever,daemon=True).start()
        url=f'http://127.0.0.1:{server.server_port}/fixture.bin'.encode()
        torrent={b'info':{b'name':b'fixture.bin',b'length':len(original),b'piece length':131072,b'pieces':hashlib.sha1(original).digest()},b'url-list':[url]}
        path=temporary/'fixture.torrent';path.write_bytes(bencode(torrent))
        try:
            result=subprocess.run([str(ROOT/'third_party/tools/aria2c.exe'),'--no-conf=true','--enable-color=false','--enable-dht=false','--enable-dht6=false','--enable-peer-exchange=false','--seed-time=0','--bt-stop-timeout=12','--timeout=5','--dir='+str(temporary/'download'),str(path)],capture_output=True,timeout=25)
            if result.returncode: raise RuntimeError(result.stdout.decode(errors='replace')+result.stderr.decode(errors='replace'))
            assert (temporary/'download/fixture.bin').read_bytes()==original,'Downloaded bytes differ'
            print('PASS BitTorrent webseed download: piece hash verified and exact output bytes')
        finally: server.shutdown();server.server_close()

if __name__=='__main__':main()
