require 'webrick'

port = ARGV[0] ? ARGV[0].to_i : 18080
server = WEBrick::HTTPServer.new(
  Port: port,
  BindAddress: '127.0.0.1',
  Logger: WEBrick::Log.new("/dev/null"),
  AccessLog: []
)

server.mount_proc '/' do |req, res|
  res.status = 200
  if req.body
    res.body = req.body
  end
end

trap 'INT' do server.shutdown end
server.start
