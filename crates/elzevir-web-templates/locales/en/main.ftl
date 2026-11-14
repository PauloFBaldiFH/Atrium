# Elzevir interface texts, English (primary language).
# Keys: <area>-<page>-<element>. Every key used in a template must exist here
# (checked by the `every_template_key_exists` test).

## Shared
app-name = Elzevir
nav-catalog = Catalog
nav-staff = Staff

## OPAC
opac-home-title = Library catalog
opac-home-search-label = Search the catalog
opac-home-search-placeholder = Title, author or subject
opac-home-search-button = Search

## Staff login
staff-login-title = Staff sign in
staff-login-user = Username
staff-login-password = Password
staff-login-submit = Sign in
staff-login-invalid = Invalid username or password.
staff-login-not-staff = This account cannot use the staff interface.
staff-login-unavailable = Sign in is temporarily unavailable. Try again shortly.

## Staff home
staff-home-title = Staff interface
staff-home-signed-in-as = Signed in as
staff-home-administration = Administration
staff-home-logout = Sign out

## OPAC results
opac-results-title = Search results
opac-results-count = { $count ->
    [one] 1 result
   *[other] { $count } results
}
opac-results-none = Nothing found for “{ $query }”.
opac-results-pages = Result pages
opac-results-previous = Previous
opac-results-next = Next
opac-availability-available = { $available } of { $items } available
opac-availability-none = No copies available
opac-availability-no-items = No copies

## OPAC record
opac-record-contributors = Authors
opac-record-material = Type
opac-record-publication = Publication
opac-record-classification = Classification
opac-record-subjects = Subjects
opac-record-identifiers = Identifiers
opac-record-languages = Languages
opac-record-summary = Summary
opac-record-audience = Audience
opac-record-contents = Contents
opac-record-notes = Notes
opac-record-items = Copies
opac-record-no-items = This record has no copies.
opac-record-item-branch = Library
opac-record-item-location = Shelf
opac-record-item-call-number = Call number
opac-record-item-status = Status
opac-not-found-title = Record not found
opac-not-found-back = Back to the catalog

## Item status
item-status-available = Available
item-status-not-for-loan = Not for loan
item-status-damaged = Damaged
item-status-lost = Lost
item-status-withdrawn = Withdrawn
item-status-restricted = Restricted

## Categories of material
material-book = Book
material-periodical = Periodical
material-map = Map
material-score = Score
material-sound-recording = Sound recording
material-video = Video
material-digital = Digital resource
material-visual-material = Visual material
material-mixed = Mixed materials

# Staff catalog listing
staff-home-catalog = Catalog
staff-catalog-title = Catalog
staff-catalog-search-label = Search the catalog
staff-catalog-search-placeholder = Title, author, subject, ISBN, call number, barcode
staff-catalog-search-button = Search
staff-catalog-next = More records
staff-catalog-empty = Nothing to show.
staff-catalog-count = { $count ->
    [one] One record found
   *[other] { $count } records found
}
staff-catalog-more-items = { $count ->
    [one] +1 item — see all
   *[other] +{ $count } items — see all
}
shelf-signal-available = Available
shelf-signal-not-for-loan = Not for loan
shelf-signal-out-of-circulation = Out of circulation
staff-cord-work = work
staff-cord-edition = edition
staff-cord-items = items
staff-catalog-legend = Item status
staff-column-hub = Hub
staff-catalog-columns = Columns shown
staff-column-authorities = Authorities
staff-column-work = Work
staff-column-instance = Instance
staff-column-items = Items
staff-badge-hub = Hub
staff-badge-agent = Agent
staff-field-authorized-name = Authorized name
staff-cord-written-by = written by
staff-badge-work = Work
staff-badge-instance = Instance
staff-field-uniform-title = Uniform title
staff-field-work-title = Work title
staff-field-agents = Agents
staff-field-classification = Classification
staff-field-subjects = Subjects
staff-field-title = Title
staff-field-imprint = Imprint
staff-field-isbn = ISBN
staff-field-material = Material
