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
    'marc21_framework.json'       => marc21_framework($koha_dir),
    'iso5426.json'                => iso5426($koha_dir),
    'search_fields.json'          => search_fields($koha_dir),
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

# Koha: the default MARC21 bibliographic framework, as the installer loads it into
# marc_tag_structure and marc_subfield_structure (frameworkcode ''), plus the maxlength
# updates in its sql_statements. The ACQ copy is left out.
sub marc21_framework {
    my ($dir) = @_;
    my $file = 'installer/data/mysql/en/marcflavour/marc21/mandatory/marc21_framework_DEFAULT.yml';
    my $source = slurp("$dir/$file");

    my ( %tags, %subfields, $table, $row );
    my $finish = sub {
        return unless $row && $table;
        return if ( $row->{frameworkcode} // '' ) ne '';
        if ( $table eq 'marc_tag_structure' ) {
            $tags{ $row->{tagfield} } = $row;
        } elsif ( $table eq 'marc_subfield_structure' ) {
            $subfields{ $row->{tagfield} }{ $row->{tagsubfield} } = $row;
        }
        undef $row;
    };
    my ($tables) = $source =~ /^tables:\n(.*?)^\S/ms or die "no tables in $file\n";
    for my $line ( split /\n/, $tables ) {
        next if $line =~ /^\s*(#.*)?$/;
        if ( $line =~ /^  - (\w+):\s*$/ ) {
            $finish->();
            $table = $1;
        } elsif ( $line =~ /^\s+(- )?(\w+):\s*(.*?)\s*$/ ) {
            my ( $starts_row, $key, $value ) = ( $1, $2, $3 );
            next if $key eq 'rows' || $key eq 'translatable' || $key eq 'multiline';
            if ($starts_row) { $finish->(); $row = {} }
            die "value outside a row in $file: $line\n" unless $row;
            $row->{$key} = yaml_scalar($value);
        } else {
            die "unexpected line in $file: $line\n";
        }
    }
    $finish->();

    my %maxlength;
    while ( $source =~ /UPDATE marc_subfield_structure SET maxlength=(\d+) WHERE tagfield='(\d{3})';/g ) {
        $maxlength{$2} = 0 + $1;
    }

    my $int  = sub { defined $_[0] && $_[0] ne '' ? 0 + $_[0] : undef };
    my $text = sub { defined $_[0] && $_[0] ne '' ? $_[0] : undef };
    my @fields;
    for my $tag ( sort keys %tags ) {
        my $t = $tags{$tag};
        my @codes = sort { ( $a =~ /\d/ ) <=> ( $b =~ /\d/ ) or $a cmp $b } keys %{ $subfields{$tag} // {} };
        push @fields, {
            tag              => $tag,
            label            => $t->{liblibrarian},
            label_opac       => $t->{libopac},
            repeatable       => JSON::PP::true_or_false( $t->{repeatable} ),
            mandatory        => JSON::PP::true_or_false( $t->{mandatory} ),
            authorised_value => $text->( $t->{authorised_value} ),
            subfields        => [
                map {
                    my $s = $subfields{$tag}{$_};
                    {
                        code             => $_,
                        label            => $s->{liblibrarian},
                        label_opac       => $s->{libopac},
                        repeatable       => JSON::PP::true_or_false( $s->{repeatable} ),
                        mandatory        => JSON::PP::true_or_false( $s->{mandatory} ),
                        kohafield        => $text->( $s->{kohafield} ),
                        tab              => $int->( $s->{tab} ),
                        hidden           => $int->( $s->{hidden} ),
                        authorised_value => $text->( $s->{authorised_value} ),
                        authtypecode     => $text->( $s->{authtypecode} ),
                        value_builder    => $text->( $s->{value_builder} ),
                        isurl            => JSON::PP::true_or_false( $s->{isurl} ),
                        maxlength        => $maxlength{$tag} // 9999,
                    }
                } @codes
            ],
        };
    }
    my @orphans = grep { !$tags{$_} } sort keys %subfields;
    die "subfields without a tag in $file: @orphans\n" if @orphans;
    return {
        source => { file => $file, sha256 => sha256_hex($source) },
        fields => \@fields,
    };
}

# Koha: the %chars table of C4::Charset (used by char_decode5426): ISO 5426 code, one
# byte or a diacritic byte followed by a base byte, to a Unicode code point.
sub iso5426 {
    my ($dir) = @_;
    my $file = 'C4/Charset.pm';
    my $source = slurp("$dir/$file");
    my %chars;
    while ( $source =~ /^\$chars\{0x([0-9a-f]+)\}\s*=\s*0x([0-9a-f]+);/mg ) {
        die "duplicate ISO 5426 code 0x$1 in $file\n" if exists $chars{ hex $1 };
        $chars{ hex $1 } = hex $2;
    }
    die "no %chars entries in $file\n" unless %chars;
    return {
        source => { file => $file, sha256 => sha256_hex($source) },
        chars  => [ map { { iso5426 => sprintf( '%04x', $_ ), unicode => sprintf( '%04x', $chars{$_} ) } } sort { $a <=> $b } keys %chars ],
    };
}

# Koha: the default search field mappings (admin/searchengine/elasticsearch/mappings.yaml):
# each search field with its type and the MARC fields that feed it, per MARC flavour, with
# the defaults Koha::SearchEngine::Elasticsearch::reset_elasticsearch_mappings applies
# (opac and staff_client default to 1, sort to 1, facet and suggestible to 0).
sub search_fields {
    my ($dir) = @_;
    my $file = 'admin/searchengine/elasticsearch/mappings.yaml';
    my $source = slurp("$dir/$file");
    my ( %indexes, $index, $field, $mapping );
    for my $line ( split /\n/, $source ) {
        next if $line =~ /^(---)?\s*$/;
        if ( $line =~ /^(\w+):\s*$/ ) {
            $index = $1;
            $indexes{$index} = {};
        } elsif ( $line =~ /^  ([\w-]+):\s*$/ ) {
            die "field outside an index in $file\n" unless $index;
            $field = $indexes{$index}{$1} = { name => $1, mappings => [] };
            undef $mapping;
        } elsif ( $line =~ /^    (\w+):\s*(.*?)\s*$/ ) {
            die "attribute outside a field in $file: $line\n" unless $field;
            undef $mapping;
            next if $1 eq 'mappings';
            $field->{$1} = yaml_scalar($2);
        } elsif ( $line =~ /^\s{4,6}- (\w+):\s*(.*?)\s*$/ ) {
            die "mapping outside a field in $file: $line\n" unless $field;
            $mapping = { $1 => yaml_scalar($2) };
            push @{ $field->{mappings} }, $mapping;
        } elsif ( $line =~ /^\s{6,8}(\w+):\s*(.*?)\s*$/ ) {
            die "key outside a mapping in $file: $line\n" unless $mapping;
            $mapping->{$1} = yaml_scalar($2);
        } else {
            die "unexpected line in $file: $line\n";
        }
    }
    my $flag = sub { JSON::PP::true_or_false( defined $_[0] && $_[0] ne '' && $_[0] ne '0' ) };
    my %out;
    for my $name ( sort keys %indexes ) {
        $out{$name} = [
            map {
                my $f = $_;
                {
                    name         => $f->{name},
                    label        => $f->{label},
                    type         => ( defined $f->{type} && $f->{type} ne '' ? $f->{type} : undef ),
                    opac         => $flag->( $f->{opac}         // 1 ),
                    staff_client => $flag->( $f->{staff_client} // 1 ),
                    mandatory    => $flag->( $f->{mandatory} ),
                    facet_order  => ( defined $f->{facet_order} ? 0 + $f->{facet_order} : undef ),
                    mappings     => [
                        map {
                            {
                                marc_type   => $_->{marc_type},
                                marc_field  => $_->{marc_field},
                                facet       => $flag->( $_->{facet} ),
                                sort        => $flag->( $_->{sort} // 1 ),
                                suggestible => $flag->( $_->{suggestible} ),
                            }
                        } sort { $a->{marc_type} cmp $b->{marc_type} or $a->{marc_field} cmp $b->{marc_field} } @{ $f->{mappings} }
                    ],
                }
            } map { $indexes{$name}{$_} } sort keys %{ $indexes{$name} }
        ];
    }
    return {
        source  => { file => $file, sha256 => sha256_hex($source) },
        indexes => \%out,
    };
}

# A YAML scalar as the framework files write them: empty, bare or double-quoted.
sub yaml_scalar {
    my ($value) = @_;
    return undef if $value eq '' || $value eq '~' || $value eq 'null';
    if ( $value =~ /^"(.*)"$/ ) {
        my $inner = $1;
        $inner =~ s/\\(["\\])/$1/g;
        return $inner;
    }
    if ( $value =~ /^'(.*)'$/ ) {
        my $inner = $1;
        $inner =~ s/''/'/g;
        return $inner;
    }
    die "unsupported YAML scalar: $value\n" if $value =~ /^[\[{&*!|>]/;
    return $value;
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
