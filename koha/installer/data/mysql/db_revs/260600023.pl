use Modern::Perl;
use Koha::Installer::Output qw(say_warning say_success say_info);

return {
    bug_number  => "42344",
    description => "Remove system preference 'CircSidebar'",
    up          => sub {
        my ($args) = @_;
        my ( $dbh, $out ) = @$args{qw(dbh out)};

        $dbh->do(
            q{
            DELETE FROM systempreferences
            WHERE variable="CircSidebar"
        }
            ) == 1
            && say_success( $out, "Removed system preference 'CircSidebar'" );
    },
};
