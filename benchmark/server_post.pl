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
) or die "Cannot create socket: $!\n";

while (my $client = $server->accept()) {
    my $pid = fork();
    if (!defined $pid) {
        die "Cannot fork: $!";
    } elsif ($pid == 0) {
        # Child process
        $server->close();
        
        my $content_length = 0;
        while (my $line = <$client>) {
            $line =~ s/\r?\n$//;
            last if $line eq "";
            if ($line =~ /^Content-Length:\s*(\d+)/i) {
                $content_length = $1;
            }
        }
        
        my $body = "";
        if ($content_length > 0) {
            read($client, $body, $content_length);
        }
        
        my $response = "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: " . length($body) . "\r\n\r\n" . $body;
        print $client $response;
        $client->close();
        exit 0;
    } else {
        # Parent process
        $client->close();
    }
}
