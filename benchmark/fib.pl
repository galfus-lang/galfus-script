use Time::HiRes qw(time);

sub fib {
    my $n = shift;
    return $n if $n <= 1;
    return fib($n - 1) + fib($n - 2);
}

my $start = time();
my $result = fib(35);
my $end = time();

my $elapsed_ms = int(($end - $start) * 1000);
print "RESULT=$result\n";
print "TIME_MS=$elapsed_ms\n";
