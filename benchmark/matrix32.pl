use strict;
use warnings;
use Time::HiRes qw(time);

my $ITERATIONS = 1_000_000;
my $MODULUS = 1009;

sub matrix4_i32 {
    my @a = (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);
    my @b = (1, 2, 3, 4, 2, 1, 4, 3, 3, 4, 1, 2, 4, 3, 2, 1);
    my @next = (0) x 16;
    my $checksum = 0;
    
    for my $iteration (1..$ITERATIONS) {
        for my $row (0..3) {
            my $offset = $row * 4;
            for my $column (0..3) {
                $next[$offset + $column] = ($a[$offset] * $b[$column] + $a[$offset + 1] * $b[$column + 4] + $a[$offset + 2] * $b[$column + 8] + $a[$offset + 3] * $b[$column + 12]) % $MODULUS;
            }
        }
        my @temp = @a;
        @a = @next;
        @next = @temp;
        $checksum = ($checksum + $a[0] + $a[5] + $a[10] + $a[15]) % $MODULUS;
    }
    return $checksum;
}

sub matrix4_f32 {
    my @a = (1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0);
    my @b = (1.0, 2.0, 3.0, 4.0, 2.0, 1.0, 4.0, 3.0, 3.0, 4.0, 1.0, 2.0, 4.0, 3.0, 2.0, 1.0);
    my @next = (0.0) x 16;
    my $checksum = 0.0;
    
    for my $iteration (1..$ITERATIONS) {
        for my $row (0..3) {
            my $offset = $row * 4;
            for my $column (0..3) {
                $next[$offset + $column] = ($a[$offset] * $b[$column] + $a[$offset + 1] * $b[$column + 4] + $a[$offset + 2] * $b[$column + 8] + $a[$offset + 3] * $b[$column + 12]) % $MODULUS;
            }
        }
        my @temp = @a;
        @a = @next;
        @next = @temp;
        $checksum = ($checksum + $a[0] + $a[5] + $a[10] + $a[15]) % $MODULUS;
    }
    return int($checksum);
}

my $start = time();
my $integer_result = matrix4_i32();
my $float_result = matrix4_f32();
my $end = time();

my $elapsed_ms = int(($end - $start) * 1000);
print "RESULT=$integer_result,$float_result\n";
print "TIME_MS=$elapsed_ms\n";
