import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def log_message(self, format, *args):
        pass


port = int(sys.argv[1]) if len(sys.argv) > 1 else 18080
ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
