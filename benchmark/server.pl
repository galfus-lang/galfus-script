use strict;
use warnings;
use IO::Socket::INET;

my $port = $ARGV[0] || 18080;
my $server = IO::Socket::INET->new(
    LocalAddr => '127.0.0.1',
    LocalPort => $port,
    Proto => 'tcp',
    Listen => 1024,
    ReuseAddr => 1
) or die "Cannot start server on port $port: $!";

while (my $client = $server->accept()) {
    my $pid = fork();
    die "Cannot fork" unless defined $pid;
    if ($pid == 0) {
        $server->close();
        while (<$client>) {
            last if /^\s*$/;
        }
        print $client "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        $client->close();
        exit(0);
    } else {
        $client->close();
    }
}
