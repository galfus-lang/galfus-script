use strict;
use warnings;
use Time::HiRes qw(time);

my $ITERATIONS = 1000000;
my $INTEGER_MODULUS = 1000000007;
my $FLOAT_MODULUS = 1000000007;

sub matrix64_i64 {
    my @a = (1000000000, 1000000001, 1000000002, 1000000003, 1000000004, 1000000005, 1000000006, 1000000007, 1000000008, 1000000009, 1000000010, 1000000011, 1000000012, 1000000013, 1000000014, 1000000015);
    my @b = (1000000001, 1000000002, 1000000003, 1000000004, 1000000002, 1000000001, 1000000004, 1000000003, 1000000003, 1000000004, 1000000001, 1000000002, 1000000004, 1000000003, 1000000002, 1000000001);
    my @next = (0) x 16;
    my $checksum = 0;
    
    for my $iteration (1..$ITERATIONS) {
        for my $row (0..3) {
            my $offset = $row * 4;
            for my $column (0..3) {
                $next[$offset + $column] = ($a[$offset] * $b[$column] + $a[$offset + 1] * $b[$column + 4] + $a[$offset + 2] * $b[$column + 8] + $a[$offset + 3] * $b[$column + 12]) % $INTEGER_MODULUS;
            }
        }
        my @temp = @a;
        @a = @next;
        @next = @temp;
        $checksum = ($checksum + $a[0] + $a[5] + $a[10] + $a[15]) % $INTEGER_MODULUS;
    }
    return $checksum;
}

sub matrix64_f64 {
    my @a = (1000000000.0, 1000000001.0, 1000000002.0, 1000000003.0, 1000000004.0, 1000000005.0, 1000000006.0, 1000000007.0, 1000000008.0, 1000000009.0, 1000000010.0, 1000000011.0, 1000000012.0, 1000000013.0, 1000000014.0, 1000000015.0);
    my @b = (1000000001.0, 1000000002.0, 1000000003.0, 1000000004.0, 1000000002.0, 1000000001.0, 1000000004.0, 1000000003.0, 1000000003.0, 1000000004.0, 1000000001.0, 1000000002.0, 1000000004.0, 1000000003.0, 1000000002.0, 1000000001.0);
    my @next = (0.0) x 16;
    my $checksum = 0.0;
    
    for my $iteration (1..$ITERATIONS) {
        for my $row (0..3) {
            my $offset = $row * 4;
            for my $column (0..3) {
                $next[$offset + $column] = ($a[$offset] * $b[$column] + $a[$offset + 1] * $b[$column + 4] + $a[$offset + 2] * $b[$column + 8] + $a[$offset + 3] * $b[$column + 12]) % $FLOAT_MODULUS;
            }
        }
        my @temp = @a;
        @a = @next;
        @next = @temp;
        $checksum = ($checksum + $a[0] + $a[5] + $a[10] + $a[15]) % $FLOAT_MODULUS;
    }
    return int($checksum);
}

my $start = time();
my $integer_result = matrix64_i64();
my $float_result = matrix64_f64();
my $end = time();

my $elapsed_ms = int(($end - $start) * 1000);
print "RESULT=$integer_result,$float_result\n";
print "TIME_MS=$elapsed_ms\n";
