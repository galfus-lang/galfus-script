import com.sun.net.httpserver.HttpServer;
import java.net.InetSocketAddress;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.ByteArrayOutputStream;

public final class ServerPost {
    private ServerPost() {}

    public static void main(String[] args) throws Exception {
        int port = args.length == 0 ? 18080 : Integer.parseInt(args[0]);
        HttpServer server = HttpServer.create(new InetSocketAddress("127.0.0.1", port), 0);
        server.createContext("/", exchange -> {
            InputStream is = exchange.getRequestBody();
            ByteArrayOutputStream buffer = new ByteArrayOutputStream();
            int nRead;
            byte[] data = new byte[1024];
            while ((nRead = is.read(data, 0, data.length)) != -1) {
                buffer.write(data, 0, nRead);
            }
            buffer.flush();
            byte[] body = buffer.toByteArray();
            
            exchange.sendResponseHeaders(200, body.length);
            OutputStream os = exchange.getResponseBody();
            os.write(body);
            os.close();
            exchange.close();
        });
        server.start();
    }
}
