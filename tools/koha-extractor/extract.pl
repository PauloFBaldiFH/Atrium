#!/usr/bin/env perl
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Koha extractor v0 (task 0.4): reads pinned Koha sources in reference/koha and
# writes JSON under data/koha-<series>/. It never loads Koha modules: each
# extracted structure is a static Perl literal, sliced from the source file and
# evaluated inside a Safe compartment that forbids I/O and system calls.
#
# Usage (from the repository root):
#   perl tools/koha-extractor/extract.pl           # write the JSON files
#   perl tools/koha-extractor/extract.pl --check   # fail if they are stale
#
# Only core Perl modules are used (JSON::PP, Digest::SHA, Safe).

use strict;
use warnings;
use utf8;

use Digest::SHA qw(sha256_hex);
use JSON::PP    ();
use Safe        ();
use File::Path  qw(make_path);

my $check = grep { $_ eq '--check' } @ARGV;

my $lock     = read_json('koha-lock.json');
my $koha_dir = 'reference/koha';
my $out_dir  = "data/koha-$lock->{series}";

-d $koha_dir or die "missing $koha_dir: run `git submodule update --init --depth 1 reference/koha`\n";
verify_pinned_version( $lock, $koha_dir );

my %outputs = (
    'circulation_rule_kinds.json' => circulation_rule_kinds($koha_dir),
    'sysprefs.json'               => sysprefs($koha_dir),
);

my $stale = 0;
for my $name ( sort keys %outputs ) {
    my $path = "$out_dir/$name";
    my $text = encode( $outputs{$name} );
    if ($check) {
        my $current = -e $path ? slurp($path) : '';
        if ( $current ne $text ) {
            warn "stale: $path (run perl tools/koha-extractor/extract.pl)\n";
            $stale = 1;
        }
    } else {
        make_path($out_dir);
        spew( $path, $text );
        print "wrote $path\n";
    }
}
exit $stale;

# Koha: Koha::CirculationRules::$RULE_KINDS and the defaults applied in
# Koha::CirculationRules::set_rule (can_be_blank = 1, is_monetary = 0).
sub circulation_rule_kinds {
    my ($dir) = @_;
    my $file = 'Koha/CirculationRules.pm';
    my $source = slurp("$dir/$file");
    my ($literal) = $source =~ /^our \$RULE_KINDS = (\{.*?^\});$/ms
      or die "cannot find \$RULE_KINDS in $file\n";
    my $kinds = Safe->new->reval($literal);
    die "cannot evaluate \$RULE_KINDS: $@\n" if $@ || ref $kinds ne 'HASH';

    my @rules;
    for my $name ( sort keys %$kinds ) {
        my $info = $kinds->{$name};
        push @rules, {
            name         => $name,
            scope        => [ @{ $info->{scope} } ],
            can_be_blank => JSON::PP::true_or_false( $info->{can_be_blank} // 1 ),
            is_monetary  => JSON::PP::true_or_false( $info->{is_monetary}  // 0 ),
        };
    }
    return {
        source => { file => $file, sha256 => sha256_hex($source) },
        rule_kinds => \@rules,
    };
}

# Koha: installer/data/mysql/mandatory/sysprefs.sql (defaults) and the staff interface's
# preference screens, koha-tmpl/intranet-tmpl/prog/en/modules/admin/preferences/*.pref
# (tab and section). Inventory only: Atrium does not copy Koha's preferences, it triages
# them into atrium.toml (task 0.11).
sub sysprefs {
    my ($dir) = @_;
    my $sql_file = 'installer/data/mysql/mandatory/sysprefs.sql';
    my $sql = slurp("$dir/$sql_file");
    my %default;
    while ( $sql =~ /^\(\s*'([^']+)'\s*,\s*'((?:[^'\\]|\\.|'')*)'/mg ) {
        $default{$1} = $2;
    }

    my $pref_dir = 'koha-tmpl/intranet-tmpl/prog/en/modules/admin/preferences';
    my %place;
    opendir my $dh, "$dir/$pref_dir" or die "cannot list $pref_dir: $!\n";
    for my $file ( sort grep { /\.pref$/ } readdir $dh ) {
        my ( $tab, $section ) = ( '', '' );
        for my $line ( split /\n/, slurp("$dir/$pref_dir/$file") ) {
            if    ( $line =~ /^(\S[^:]*):\s*$/ )        { $tab     = $1; $section = '' }
            elsif ( $line =~ /^    (\S[^:]*):\s*$/ )    { $section = $1 }
            elsif ( $line =~ /^\s*-?\s*pref:\s*(\S+)/ ) {
                $place{$1} //= { tab => $tab, section => $section, file => $file };
            }
        }
    }
    closedir $dh;

    my @prefs;
    for my $name ( sort { lc $a cmp lc $b or $a cmp $b } keys %default ) {
        my $where = $place{$name};
        push @prefs, {
            name    => $name,
            default => $default{$name},
            tab     => $where ? $where->{tab}     : undef,
            section => $where ? $where->{section} : undef,
        };
    }
    return {
        source => {
            defaults => { file => $sql_file, sha256 => sha256_hex($sql) },
            screens  => $pref_dir,
        },
        sysprefs => \@prefs,
    };
}

sub verify_pinned_version {
    my ( $lock, $dir ) = @_;
    my ($version) = slurp("$dir/Koha.pm") =~ /\$VERSION = "([^"]+)"/
      or die "cannot read \$VERSION from $dir/Koha.pm\n";
    $version eq $lock->{version}
      or die "reference/koha is $version but koha-lock.json pins $lock->{version}\n";
}

sub JSON::PP::true_or_false { $_[0] ? JSON::PP::true : JSON::PP::false }

sub encode { JSON::PP->new->canonical->pretty->indent_length(2)->encode( $_[0] ) }

sub read_json { JSON::PP->new->decode( slurp( $_[0] ) ) }

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
    close $fh or die "cannot write $path: $!\n";
}
