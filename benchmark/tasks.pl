use strict;
use warnings;
use Time::HiRes qw(time);
use IO::Pipe;
use IO::Select;

my $ITERATIONS = 200_000;
my $WORKER_COUNT = 4;

my $start_time = time();

my @pipes;
for (1..$WORKER_COUNT) {
    my $pipe = IO::Pipe->new();
    push @pipes, $pipe;
}

my $results_pipe = IO::Pipe->new();
my @pids;

for my $index (0..$WORKER_COUNT-1) {
    my $pid = fork();
    if (!defined $pid) {
        die "Cannot fork: $!";
    } elsif ($pid == 0) {
        my $in_pipe = $pipes[$index];
        my $out_pipe = $pipes[($index + 1) % $WORKER_COUNT];
        $in_pipe->reader();
        $out_pipe->writer();
        $results_pipe->writer();
        
        my $state = 1;
        for my $i (1..20) {
            for my $j (1..($ITERATIONS / 20)) {
                $state = ($state * 127 + 17) % 1000003;
            }
            $out_pipe->print("x");
            $out_pipe->flush();
            my $char;
            $in_pipe->read($char, 1);
            $state = ($state + length($char)) % 1000003;
        }
        $results_pipe->print("$state\n");
        $results_pipe->flush();
        exit(0);
    } else {
        push @pids, $pid;
    }
}

$results_pipe->reader();

my $result = 0;
for (1..$WORKER_COUNT) {
    my $val = $results_pipe->getline();
    chomp($val);
    $result += $val;
}

for my $pid (@pids) {
    waitpid($pid, 0);
}

my $elapsed_ms = int((time() - $start_time) * 1000);
print "RESULT=$result\n";
print "TIME_MS=$elapsed_ms\n";
