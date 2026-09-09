require 'socket'

port = ARGV[0] ? ARGV[0].to_i : 18080
server = TCPServer.new('127.0.0.1', port)

loop do
  Thread.start(server.accept) do |client|
    begin
      request = client.gets
      if request
        client.print "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
      end
    ensure
      client.close
    end
  end
end
