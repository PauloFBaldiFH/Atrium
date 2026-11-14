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
staff-column-hub = Hub
staff-catalog-columns = Columns shown
staff-row-shape = { $works ->
    [one] One work
   *[other] { $works } works
}, { $instances ->
    [one] one instance
   *[other] { $instances } instances
}
staff-row-available = { $available } of { $items } available
staff-catalog-focus-label = Family in focus
staff-export = Export
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

## Every field of a focused block
staff-fields-work = Work
staff-fields-instance = Instance
staff-fields-items = Items
staff-fields-preferred-title = Preferred title
staff-fields-material = Category of material
staff-fields-classification = Classification
staff-fields-languages = Languages
staff-fields-contributions = Contributions
staff-fields-primary = main entry
staff-fields-subjects = Subjects
staff-fields-summary = Summary
staff-fields-audience = Intended audience
staff-fields-relations = Related works
staff-fields-title = Title
staff-fields-edition = Edition
staff-fields-place = Place of publication
staff-fields-publisher = Publisher
staff-fields-date = Date of publication
staff-fields-extent = Extent
staff-fields-series = Series statement
staff-fields-identifiers = Identifiers
staff-fields-note-general = General note
staff-fields-note-contents = Contents
staff-fields-note-local = Local note
staff-fields-item-status = Status
staff-fields-barcode = Barcode
staff-fields-inventory-number = Inventory number
staff-fields-call-number = Call number
staff-fields-branch = Branch
staff-fields-location = Location
staff-fields-collection = Collection
staff-fields-item-type = Item type
staff-fields-copy = Copy
staff-fields-acquired = Acquired
staff-agent-person = person
staff-agent-organization = organization
staff-agent-meeting = meeting
staff-agent-family = family
staff-subject-topic = topic
staff-subject-place = place
staff-subject-agent = name
staff-subject-title = title
staff-subject-genre-form = genre/form
staff-subject-temporal = period
staff-identifier-isbn = ISBN
staff-identifier-issn = ISSN
staff-identifier-lccn = LCCN
staff-identifier-ean = EAN
staff-identifier-system-control-number = control number
staff-identifier-doi = DOI
staff-identifier-ismn = ISMN
staff-relation-translation-of = translation of
staff-relation-adaptation-of = adaptation of
staff-relation-part-of = part of
staff-relation-supplement-to = supplement to
staff-relation-continuation-of = continuation of
staff-relation-work = Work { $id }

## Editing refusals
edit-error-title-empty = The title cannot be empty.
edit-error-language-code = Use three-letter language codes, such as por or eng.
edit-error-isbn = This ISBN is not valid: check the digits.
edit-error-issn = This ISSN is not valid: check the digits.

## Editing in the focused block
edit-empty-fields = Fields not filled in yet
edit-done = Done
edit-done-hint = Ctrl+Enter
edit-saved = Saved
edit-add-row = Add
edit-remove-row = Remove
edit-discard-question = Discard your changes?
edit-discard = Discard
edit-keep = Keep editing
edit-stale = Someone else changed this record. Reload to see their changes before editing.
edit-failed = The edit could not be saved. Try again.
edit-field-notes = Notes
edit-column-name = Name
edit-column-kind = Type
edit-column-role = Role
edit-column-number = Number
edit-column-scheme = Scheme
edit-column-heading = Heading
edit-column-thesaurus = Thesaurus
edit-column-value = Value
edit-column-qualifier = Qualifier
edit-column-text = Text
