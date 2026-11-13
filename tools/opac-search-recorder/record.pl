#!/usr/bin/env perl
# SPDX-License-Identifier: GPL-3.0-or-later
# Records what the Koha OPAC finds for each reference query (task 1.8, part 2).
#
#   perl tools/opac-search-recorder/record.pl https://opac.example/ \
#       tools/opac-search-recorder/queries.txt > crates/elzevir/tests/data/koha-opac-search.tsv
#
# For each query it pages through the OPAC's RSS results (100 per page) and keeps the
# biblionumbers up to --max-biblionumber (default 200: the records of the staff-export
# fixture). Only the public OPAC is used: no login, no patron data. Output is TSV:
# query, total hits in the whole Koha catalogue, matching biblionumbers (comma-separated).
use strict;
use warnings;
use utf8;
use open qw(:std :encoding(UTF-8));
use Encode qw(encode);

# Percent-encodes UTF-8 bytes (core Perl only, no URI::Escape).
sub uri_escape_utf8 { encode('UTF-8', shift) =~ s/([^A-Za-z0-9\-._~])/sprintf('%%%02X', ord $1)/ger }

my ($base, $queries, $max) = (@ARGV, 200);
die "usage: record.pl OPAC_BASE_URL QUERIES_FILE [MAX_BIBLIONUMBER]\n" unless $base && $queries;
$base =~ s{/*$}{};

open my $in, '<:encoding(UTF-8)', $queries or die "$queries: $!\n";
my @queries = grep { length && !/^#/ } map { s/\s+$//r } <$in>;

for my $query (@queries) {
    my ($total, %found) = (undef);
    for (my $offset = 0; !defined $total || $offset < $total; $offset += 100) {
        my $url = "$base/cgi-bin/koha/opac-search.pl?format=rss&count=100&offset=$offset&q="
            . uri_escape_utf8($query);
        # `use open` already decodes the command's output as UTF-8.
        my $rss = `curl -sS --fail --max-time 60 '$url'`;
        die "request failed for '$query' at offset $offset\n" if $? != 0;
        ($total) = $rss =~ m{<opensearch:totalResults>(\d+)<} or die "no total for '$query'\n";
        my @numbers = $rss =~ m{<guid>[^<]*opac-detail\.pl\?biblionumber=(\d+)}g;
        last unless @numbers;
        $found{$_} = 1 for grep { $_ <= $max } @numbers;
        sleep 1;    # the oracle is a desktop PC; be gentle
    }
    print join("\t", $query, $total, join(',', sort { $a <=> $b } keys %found)), "\n";
}
