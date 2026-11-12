#!/usr/bin/env perl
# SPDX-License-Identifier: GPL-3.0-or-later
#
# MARC-8 extractor: reads the Library of Congress MARC-8 code tables
# (codetables.xml) and writes data/loc/marc8.json. The XML file itself is not
# versioned; this script records its URL and SHA-256 so anyone can fetch the
# same file and regenerate the JSON byte for byte.
#
# Usage (from the repository root):
#   curl -o /tmp/codetables.xml https://www.loc.gov/marc/specifications/codetables.xml
#   perl tools/marc8-extractor/extract.pl /tmp/codetables.xml           # write
#   perl tools/marc8-extractor/extract.pl /tmp/codetables.xml --check   # compare
#
# The parser is a restricted line reader for the layout of that one file (core
# Perl has no XML parser): any element it does not expect aborts the run.

use strict;
use warnings;
use utf8;

use Digest::SHA qw(sha256_hex);
use JSON::PP    ();
use File::Path  qw(make_path);

use constant SOURCE_URL => 'https://www.loc.gov/marc/specifications/codetables.xml';
use constant OUT_PATH   => 'data/loc/marc8.json';

my ($input) = grep { $_ ne '--check' } @ARGV;
my $check = grep { $_ eq '--check' } @ARGV;
defined $input or die "usage: $0 <codetables.xml> [--check]\n";

my $xml = slurp($input);
my $data = {
    source => { url => SOURCE_URL, sha256 => sha256_hex($xml) },
    character_sets => character_sets($xml),
};
my $text = JSON::PP->new->canonical->pretty->indent_length(2)->encode($data);

if ($check) {
    my $current = -e OUT_PATH ? slurp(OUT_PATH) : '';
    if ( $current ne $text ) {
        warn "stale: @{[OUT_PATH]} (run perl tools/marc8-extractor/extract.pl <codetables.xml>)\n";
        exit 1;
    }
    exit 0;
}
make_path('data/loc');
spew( OUT_PATH, $text );
print "wrote @{[OUT_PATH]}\n";

# One entry per <characterSet>, in file order, with its <code> elements.
# Hex values stay as strings; the code generator turns them into numbers.
sub character_sets {
    my ($source) = @_;
    my @sets;
    my $current_set;
    my $code;
    for my $line ( split /\n/, $source ) {
        if ( $line =~ /<characterSet\b[^>]*\bname="([^"]+)"[^>]*\bISOcode="([0-9A-F]{2})"/ ) {
            $current_set = { iso_code => $2, name => $1, codes => [] };
            push @sets, $current_set;
        } elsif ( $line =~ m{^\s*<code>\s*$} ) {
            die "<code> outside a character set\n" unless $current_set;
            die "nested <code>\n" if $code;
            $code = { combining => JSON::PP::false };
        } elsif ( $line =~ m{^\s*</code>\s*$} ) {
            die "</code> without <code>\n" unless $code;
            die "code without <marc>\n" unless defined $code->{marc};
            die "code $code->{marc} has neither <ucs> nor <alt>\n"
              unless defined $code->{ucs} || defined $code->{alt};
            push @{ $current_set->{codes} }, $code;
            undef $code;
        } elsif ( $code && $line =~ m{^\s*<isCombining>(true|false)</isCombining>\s*$} ) {
            $code->{combining} = $1 eq 'true' ? JSON::PP::true : JSON::PP::false;
        } elsif ( $code && $line =~ m{^\s*<(marc|ucs|alt)>([0-9A-F]*)</\1>\s*$} ) {
            die "duplicate <$1> in code\n" if exists $code->{$1};
            $code->{$1} = $2 if $2 ne '';
            if ( $1 eq 'marc' ) {
                die "empty <marc>\n" if $2 eq '';
            }
        }
    }
    die "unterminated <code>\n" if $code;
    die "no character sets found\n" unless @sets;
    return \@sets;
}

sub slurp {
    my ($path) = @_;
    open my $fh, '<:raw', $path or die "cannot read $path: $!\n";
    local $/;
    return scalar <$fh>;
}

sub spew {
    my ( $path, $text ) = @_;
    open my $fh, '>:raw', $path or die "cannot write $path: $!\n";
    print {$fh} $text;
    close $fh or die "cannot close $path: $!\n";
}
