import com.sun.net.httpserver.HttpServer;
import java.net.InetSocketAddress;

public final class Server {
    private Server() {}

    public static void main(String[] args) throws Exception {
        int port = args.length == 0 ? 18080 : Integer.parseInt(args[0]);
        HttpServer server = HttpServer.create(new InetSocketAddress("127.0.0.1", port), 0);
        server.createContext("/", exchange -> {
            exchange.sendResponseHeaders(200, -1);
            exchange.close();
        });
        server.start();
    }
}
