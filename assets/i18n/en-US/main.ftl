# common
common-on = On
common-off = Off
common-left = Left
common-right = Right
common-search = Search
common-unknown = Unknown
common-not-provided = Not provided
common-not-available = Not available
common-cancel = Cancel
common-save = Save
common-delete = Delete
common-play = Play
common-more = More
common-previous = Previous
common-next = Next
common-dismiss = Dismiss
common-clear = Clear
number-group = { "," }

# navigation
nav-history = History
nav-home = Home
nav-search = Search
nav-library = Your Library
nav-settings = Settings
nav-songs = Songs
nav-albums = Albums
nav-playlists = Playlists
nav-artists = Artists
nav-local = Local Music
nav-back = Back
nav-forward = Forward
nav-sidebar = Toggle sidebar
nav-sidebar-right = Show or hide lyrics and queue
nav-pinned = Pinned
nav-unpin = Unpin
nav-pin-hint = Drop here to pin
library-liked-songs = Favorites
library-play-liked-songs = Play
library-no-songs = No favorites yet
library-no-albums = No saved albums yet
library-no-playlists = No playlists yet
library-no-artists = No favorite artists yet
library-no-local-songs = No imported songs found
library-no-local-albums = No imported albums found
library-no-local-artists = No imported artists found
library-no-local-playlists = No local playlists yet
library-no-catalog-songs = No songs found
library-no-catalog-albums = No albums found
library-no-catalog-artists = No artists found
library-no-matches = No matches
library-not-loaded = Your library did not load
library-scanning = Looking through your folders…
library-scanning-found = Looking through your folders… { $found } so far
library-scanning-progress = Reading your music… { $read } of { $found }
library-part-not-loaded = This part of your library did not load
library-local-unconfigured = Configure your local library

# app menu
app-refresh = Refresh
app-sign-out = Sign Out
app-quit = Quit
app-settings = Settings…
app-hide = Hide Sonora
app-hide-others = Hide Others
app-show-all = Show All
app-edit = Edit
app-cut = Cut
app-copy = Copy
app-paste = Paste
app-select-all = Select All
app-window = Window
app-close-window = Close Window
app-minimize = Minimize
app-zoom = Zoom

# tray menu
tray-show = Show Sonora
tray-play = Play
tray-pause = Pause

# table columns
column-played-at = Played
column-index = #
column-title = Title
column-artist = Artist
column-album = Album
column-date-added = Date added
column-added-by = Added by
column-modified = Modified
column-length = Length
column-plays = Plays
column-name = Name
column-owner = Owner
column-year = Year
column-tracks = Tracks

# track menu
menu-add-to-playlist = Add to playlist
menu-add-tracks-to-playlist = { $count ->
    [one] Add { $count } track to playlist
   *[other] Add { $count } tracks to playlist
}
menu-new-playlist = New playlist
menu-edit-tags = Edit tags
menu-no-playlists = No playlists
menu-search-playlists = Search a playlist
menu-add-to-library = Add to Favorites
menu-add-tracks-to-library = { $count ->
    [one] Add { $count } track to Favorites
   *[other] Add { $count } tracks to Favorites
}
menu-remove-from-library = Remove from Favorites
menu-remove-tracks-from-library = { $count ->
    [one] Remove { $count } track from Favorites
   *[other] Remove { $count } tracks from Favorites
}
menu-remove-from-playlist = Remove from playlist
menu-remove-tracks-from-playlist = { $count ->
    [one] Remove { $count } track from playlist
   *[other] Remove { $count } tracks from playlist
}
menu-remove-from-history = Remove from history
menu-remove-tracks-from-history = { $count ->
    [one] Remove { $count } track from history
   *[other] Remove { $count } tracks from history
}
menu-delete-track-file = Delete track file
menu-delete-track-files = Delete { $count } track files
menu-download = Download
menu-download-tracks = { $count ->
    [one] Download { $count } track
   *[other] Download { $count } tracks
}
menu-remove-download = Remove download
menu-remove-downloads = { $count ->
    [one] Remove { $count } download
   *[other] Remove { $count } downloads
}
menu-downloading = Downloading. Click to stop
menu-play-next = Play next
menu-play-tracks-next = { $count ->
    [one] Play { $count } track next
   *[other] Play { $count } tracks next
}
menu-add-to-queue = Add to queue
menu-add-tracks-to-queue = { $count ->
    [one] Add { $count } track to queue
   *[other] Add { $count } tracks to queue
}
menu-play-last = Play last
menu-play-tracks-last = { $count ->
    [one] Play { $count } track last
   *[other] Play { $count } tracks last
}
menu-song-radio = Go to song radio
menu-go-to-album = Go to album
menu-go-to-artist = Go to artist
menu-view-details = View details
menu-copy-link = Copy link
menu-cut = Cut
menu-copy = Copy
menu-paste = Paste
menu-select-all = Select all
menu-remove-from-queue = Remove from queue
menu-rename-playlist = Rename playlist
menu-delete-playlist = Delete playlist
menu-add-playlist-to-library = Add to Library
menu-remove-playlist-from-library = Remove from Library
menu-library-add = Add to Library
menu-library-add-tracks = { $count ->
    [one] Add { $count } track to Library
   *[other] Add { $count } tracks to Library
}
menu-library-remove = Remove from Library
menu-library-remove-tracks = { $count ->
    [one] Remove { $count } track from Library
   *[other] Remove { $count } tracks from Library
}
menu-make-playlist-public = Make public
menu-make-playlist-private = Make private
menu-play-artist = Play artist

# playlist editor
playlist-name-placeholder = Playlist name
playlist-create-title = Create playlist
playlist-rename-title = Rename playlist
playlist-delete-title = Delete playlist
playlist-delete-confirm = Delete “{ $name }”? This cannot be undone.
playlist-again-title = Add it again?
playlist-again-confirm = This track is already in “{ $name }”. Add another copy?
playlist-again-add = Add again

# confirm
confirm-remove-library-title = Remove from library
confirm-remove-playlist-title = Remove from playlist
confirm-remove-history-title = Remove from history
confirm-remove-songs = { $count ->
    [one] Remove this song from your library?
   *[other] Remove { $count } songs from your library?
}
confirm-remove-playlist-songs = { $count ->
    [one] Remove this song from the playlist?
   *[other] Remove { $count } songs from the playlist?
}
confirm-remove-history-songs = { $count ->
    [one] Remove this song from listening history?
   *[other] Remove { $count } songs from listening history?
}
confirm-remove-albums = { $count ->
    [one] Remove this album from your library?
   *[other] Remove { $count } albums from your library?
}
confirm-remove-artists = { $count ->
    [one] Remove this artist from Favorites?
   *[other] Remove { $count } artists from Favorites?
}
confirm-remove-playlists = { $count ->
    [one] Remove this playlist from your library?
   *[other] Remove { $count } playlists from your library?
}
confirm-delete-track-files-title = Delete track files?
confirm-delete-track-files = The file(s) will be permanently deleted from disk. This cannot be undone.

# queue panel
queue-title = Queue
queue-history = History
queue-now-playing = Now playing
queue-from = From
queue-from-radio = { $name } radio
queue-next-in-queue = Next in queue
queue-up-next = Up next
queue-reset = Reset
queue-clear = Clear
queue-added-by = Added by { $name }
queue-empty = Your queue is empty
queue-similar = Similar tracks
queue-radio = Autoplay similar tracks
queue-return-playing = Back to now playing

# player bar
player-nothing-playing = Nothing playing
player-percent = { $value }%
player-shuffle = Shuffle
player-repeat = Repeat
player-repeat-all = Repeat all
player-repeat-one = Repeat one
player-mute = Mute
player-unmute = Unmute
player-previous = Previous track
player-next = Next track
player-mini = Mini player
player-fullscreen = Now playing
player-fullscreen-leave = Leave Now playing
player-devices = Devices
devices-this-computer = This computer
devices-pause = Pause
devices-resume = Resume
devices-jam-join = Join a Jam…
devices-jam-join-title = Join a Jam
devices-jam-join-detail = Paste the link a friend shared with you
devices-jam-paste = Paste
devices-jam-link-hint = Jam link
devices-jam-start = Start a Jam
devices-jam-copy = Copy Jam link
devices-jam-code = QR code
devices-jam-code-menu = Show QR code
devices-jam-code-title = Scan to join { $host }'s Jam
devices-jam-code-title-anonymous = Scan to join the Jam
devices-jam-code-close = Close
devices-jam-leave = Leave Jam
devices-jam-end = End Jam
devices-jam-banner = In { $host }'s Jam
devices-jam-banner-anonymous = In a Jam
devices-jam-more = +{ $count }
devices-jam-host = Host
devices-jam-you = You
devices-jam-remove = Remove
devices-jam-invite-title = Join { $host }'s Jam?
devices-jam-invite-title-anonymous = Join this Jam?
devices-jam-invite-detail = { $count ->
    [one] { $count } person is already in this Jam.
   *[other] { $count } people are already in this Jam.
}
devices-jam-invite-join = Join
devices-elsewhere-banner = Playing on { $device }
devices-elsewhere-here = Play here
player-video-on = Show video
player-video-off = Hide video
video-pip = Picture in picture
video-large = Fullscreen
video-back = Back to Sonora
fullscreen-artwork = Artwork

# filters
filter-history = Filter listening history
history-empty = Tracks you play will appear here.
history-not-loaded = Listening history could not be loaded.
history-clear = Clear history
history-clear-title = Clear listening history
history-clear-confirm = Every play is removed from this device. This cannot be undone.
filter-library = Filter your library
filter-album = Filter album tracks
filter-reset = Reset filters
filter-duration = Duration
filter-year = Year
filter-hide-explicit = Hide explicit
filter-playable = Playable only
filter-favorites = Favorites only
filter-downloaded = Downloaded
filter-owned = By you

# view
view-list = List
view-cards = Grid

# toolbar
tool-columns = Columns
tool-sort = Sort
tool-filters = Filters

# login
login-signed-out = Sign in to load your music library
login-restoring = Checking your saved session…
login-authorizing = Waiting for authorization in your browser…
login-signed-in = Signed in as { $name }
login-failed-title = Sign-in failed
login-problem-region = { $provider } will not open a session from the country you are in. Sign in from your home country, or change the country on your { $provider } account.
login-problem-credentials = Your saved { $provider } session is no longer valid. Sign in again to continue.
login-problem-secret = { $provider } did not accept what you pasted. Copy the value again and make sure you took all of it.
login-problem-network = Sonora could not reach { $provider }. Check your internet connection and try again.
login-problem-cancelled = The sign-in was not approved in the browser, or ran out of time. Start again to finish.
login-problem-refused = { $provider } turned down the sign-in. Wait a moment and try again.
login-problem-premium = Sonora streams through Spotify Premium, and this account does not have it. Sign in with a Premium account to continue.
login-sign-in = Sign in with { $provider }
login-connect-cookies = Paste cookies manually
login-cookie-open = Open { $provider }
login-connect-token = Paste an OAuth token manually
login-import-browser = Import from browser*
login-import-browser-plain = Import from browser
login-browser-firefox = *Firefox-based browsers only
login-browser-title = Choose a browser
login-browser-detail = Sonora reads your { $provider } session from the browser you pick. The session stays on this device.
login-use = Use { $provider }
login-guest-title = Guest mode
login-guest-use = Use Guest mode
login-guest-detail = Browse and play without an account. Your library, likes and playlists stay out of reach.
login-usage-consent = Help us estimate how many people use Sonora.
login-device-code = Enter this code at { $url }
login-cookie-submit = Continue
login-cookie-hint = Paste the Cookie request header here
login-cookie-step-1 = Open music.youtube.com and make sure you are signed in. An incognito window works best.
login-cookie-video-step-1 = Open www.youtube.com and make sure you are signed in. An incognito window works best.
login-cookie-step-2 = Press F12, open the Network tab and reload the page.
login-cookie-step-3 = Select any request named "browse" or "next".
login-cookie-step-4 = In Headers, find Cookie under Request Headers, right-click it and copy its value.
login-cookie-step-note = Make sure to paste the whole value, including SAPISID and __Secure-3PAPISID.
login-cookie-header-title = Paste your { $provider } cookies to finish signing in
login-cookie-apple-step-3 = Select any request to { $site }.
login-cookie-apple-note = Make sure to paste the whole value, including { $cookie }.
login-cookie-named-title = Paste your { $provider } cookie to finish signing in
login-cookie-named-hint = Paste the cookie value here
login-cookie-named-step-1 = Open { $site } and make sure you are signed in.
login-cookie-named-step-2 = Press F12 and open the Storage tab in Firefox, or the Application tab in Chrome.
login-cookie-named-step-3 = Expand Cookies, select { $site } and find the cookie named { $cookie }.
login-cookie-named-step-4 = Double-click its value and copy it.
login-cookie-named-note = The { $cookie } value alone is enough. A whole Cookie header copied from the Network tab works too.
login-window-title = Sign in to { $provider }
login-server-title = Connect to your Subsonic server
login-server-detail = Enter the address of any Subsonic or OpenSubsonic server (Navidrome, Airsonic, Gonic, …), then sign in with your server username and password. The session stays on this device.
login-server-hint = https://music.example.com
login-username-hint = Username
login-password-hint = Password
login-server-submit = Connect
login-cookie-title = Paste your { $provider } cookies to finish signing in
login-token-title = Paste your SoundCloud OAuth token to finish signing in
login-token-step-1 = Open soundcloud.com and make sure you are signed in.
login-token-step-2 = Press F12, open the Network tab and reload the page.
login-token-step-3 = Type me in the filter box and select the request named me.
login-token-step-4 = Under Request Headers, copy the whole Authorization value.
login-token-step-note = The leading "OAuth " is fine to include, Sonora removes it.
login-token-hint = Paste the OAuth token here
login-secret-open = Open { $site }
login-account-title = Choose an account
login-account-detail = This session is signed in to more than one Google account. Pick the one Sonora should use.
login-choose-title = Sign in to { $provider }
login-choose-detail = Choose how to sign in to { $provider }.

# album and playlist pages
detail-playlist = Playlist
detail-play-album = Play album
detail-play-playlist = Play playlist

# play button
play-pause = Pause
play-resume = Resume
play-shuffle = Shuffle

# artist page
artist-eyebrow = Artist
artist-monthly-listeners = { $count ->
    [one] { $value } monthly listener
   *[other] { $value } monthly listeners
}
artist-play = Play now
artist-popular = Popular
artist-popular-eyebrow = Explore this artist
artist-popular-empty = Nothing to play from this artist yet
artist-popular-more = Show all
artist-popular-less = Show less
artist-releases = Releases
artist-releases-more = Show all
artist-releases-less = Show less
artist-filter-all = All
artist-filter-albums = Albums
artist-filter-singles = Singles
artist-filter-eps = EPs
artist-appears-on = Appears on
album-also-like = You might also like
album-tab-albums = Albums
album-tab-artists = Artists
album-label = ℗ { $label }

# user profile page
user-eyebrow = Profile
user-followers = { $count ->
    [one] { $value } follower
   *[other] { $value } followers
}
user-following = { $count ->
   *[other] { $value } following
}
user-playlists = Public playlists
user-playlists-empty = No public playlists yet

# release kinds
release-album = Album
release-single = Single
release-compilation = Compilation
release-ep = EP
release-audiobook = Audiobook
release-podcast = Podcast
release-meta = { $year } • { $kind }

# home page
home-quick-picks = Quick picks
home-quick-picks-empty = Like a few songs and they will show up here
home-featured-release = New release · { $provider }
home-featured-mix = Mix
home-featured-play = Play
home-featured-open = Open album
home-resume = Jump back in
home-new-releases = New from your artists
home-because = Because you listened to { $artist }
home-recently-added = Recently added
home-playlists = Playlists
home-favorite-albums = Favorite albums
home-artists = Artists
home-collection-albums = Albums from your collection

# search page
search-placeholder = What do you want to listen to?
search-browse = Browse all
search-recent = Recent searches
search-recent-clear = Clear
search-recent-remove = Remove from recent searches
genre-empty = Nothing to show here yet
search-best-match = Best match
search-no-matches = No matches
search-results = Results
search-songs = Songs
search-artists = Artists
search-albums-playlists = Albums & playlists
search-all = All
search-albums = Albums
search-playlists = Playlists
search-mine-only = Only in your library
search-filtered-empty = Nothing matches these filters
search-tag = { $kind } ·
search-saved =
    { $count ->
        [one] { $count } song in Library
       *[other] { $count } songs in Library
    }
kind-song = Song
kind-artist = Artist
kind-album = Album
kind-playlist = Playlist

# song page
song-eyebrow = Song
song-play = Play song
song-view-album = View album
song-loading = Loading song information…
song-about = About this song
song-album = Album
song-released = Released
song-streams = Streams
song-position = Position
song-label = Label
song-popularity = Popularity
song-popularity-value = { $value }%
song-disc-track = Disc { $disc }, track { $track }
song-track = Track { $track }
song-credits = Credits
song-performed-by = Performed by
song-details = Genres & details
song-genres = Genres
song-language = Language
song-content = Content
song-explicit = Explicit
song-clean = Clean
artist-about = About the artist
artist-about-fallback = Explore the artist's popular songs and releases.
artist-about-open = Go to artist
song-copyright = © { $notice }

# song languages
language-ar = Arabic
language-de = German
language-en = English
language-es = Spanish
language-fr = French
language-hi = Hindi
language-it = Italian
language-ja = Japanese
language-ko = Korean
language-pt = Portuguese
language-ru = Russian
language-tr = Turkish
language-uk = Ukrainian
language-zh = Chinese
language-zxx = No linguistic content

# counts
count-songs =
    { $count ->
        [one] { $count } song
       *[other] { $count } songs
    }
count-tracks =
    { $count ->
        [one] { $count } track
       *[other] { $count } tracks
    }

# running times
runtime-seconds = { $seconds }s
runtime-minutes = { $minutes }m { $seconds }s
runtime-hours = { $hours }h { $minutes }m

# dates
date-just-now = Just now
date-minute-ago = A minute ago
date-minutes-ago = { $count } minutes ago
date-today = Today at { $time }
date-yesterday = Yesterday at { $time }
date-time = { $date }, { $time }
date-full = { $month } { $day }, { $year }
month-1 = Jan
month-2 = Feb
month-3 = Mar
month-4 = Apr
month-5 = May
month-6 = Jun
month-7 = Jul
month-8 = Aug
month-9 = Sep
month-10 = Oct
month-11 = Nov
month-12 = Dec

# settings
settings-search = Search settings
settings-search-results = Search results
# where a search result lives: a drill-in page under the page it opens from
settings-result-place = { $parent } › { $page }
settings-page-providers = Providers
settings-page-appearance = Appearance
settings-page-player-bar = Player bar
settings-player-bar-layout = Layout
settings-player-bar-layout-detail = Arrange the track information, playback controls and progress bar
settings-player-bar-classic = Classic
settings-player-bar-compact = Compact
settings-player-bar-centered = Centered
settings-player-bar-surface = Surface
settings-player-bar-transparent = Transparent player bar
settings-player-bar-transparent-detail = Let pages and sidebars scroll underneath a frosted player bar
settings-player-bar-transparent-needs-blur = Turn on Blur UI to let pages scroll underneath a frosted player bar
settings-page-now-playing = Now playing
settings-page-lyrics = Lyrics
settings-page-playback = Playback
settings-page-sound = Sound
settings-page-storage = Storage
settings-page-discord = Discord
settings-page-scrobbling = Scrobbling
settings-page-general = General
settings-page-about = About
settings-tab-integrations = Integrations
settings-about-name = Sonora
settings-page-lyrics-sources = Lyrics sources
settings-page-music-folders = Music folders
settings-group-services = Streaming services
settings-group-local-files = Local files
settings-group-background = Background
settings-group-controls = Controls
settings-group-display = Display
settings-group-sources = Sources
settings-group-lyrics-order = Priority
settings-group-playing = Playing
settings-group-cache = Played tracks
settings-group-downloads = Downloads
settings-group-protected = Protected playback
settings-group-output = Output
settings-group-status = Status
settings-group-startup = Startup
settings-group-context-menus = Context menus
key-ctrl = Ctrl
key-super = Super
key-alt = Alt
key-shift = Shift
key-escape = Esc
key-enter = Enter
key-space = Space
key-tab = Tab
key-backspace = Backspace
key-delete = Delete
settings-lyrics-sources = Lyrics sources
settings-local-folder-count = { $count ->
    [one] { $count } folder
   *[other] { $count } folders
    }
settings-local-folder-none = No folders yet
settings-license-short = GPL-3.0 or later
settings-theme = Theme
settings-theme-detail = Choose the application colour palette
settings-opacity = Opacity
settings-opacity-detail = Adjust the app background opacity
settings-opacity-value = { $percent }%
settings-theme-folder = Open folder
settings-theme-unavailable = This theme can be applied only when adaptive theme is disabled
settings-adaptive = Adaptive theme
settings-adaptive-detail = Tint the palette with the artwork of the playing album
settings-ambient = Ambient background
settings-ambient-detail = Fill Now playing with colours drawn from the cover
settings-ambient-motion = Ambient motion
settings-ambient-motion-detail = Drift the ambient colours instead of holding them still
settings-canvas = Canvas
settings-waveform = Waveform seek bar
settings-waveform-detail = Draws a song's loudness along the seek bar, so you can see where the quiet and loud parts are
settings-waveform-style = Waveform style
settings-waveform-style-detail = How the waveform is drawn along the seek bar
settings-waveform-style-mirrored = Mirrored bars
settings-waveform-style-bars = Bars
settings-waveform-style-smooth = Smooth
settings-waveform-density = Waveform density
settings-waveform-density-detail = How finely the waveform is cut into bars
settings-waveform-density-fine = Fine
settings-waveform-density-normal = Normal
settings-waveform-density-coarse = Coarse
settings-waveform-bar = Waveform in the player bar
settings-waveform-bar-detail = Draws the waveform in the seek bar at the bottom of the window
settings-waveform-fullscreen = Waveform in fullscreen
settings-waveform-fullscreen-detail = Draws the waveform in the seek bar of the fullscreen player
settings-waveform-mini = Waveform in the mini player
settings-waveform-mini-detail = Draws the waveform in the seek bar of the mini player
settings-waveform-measure = Measure waveforms
settings-waveform-measure-detail = Reads a song a second time once it has downloaded to draw its waveform. Off saves that work and shows only the waveforms SoundCloud provides
settings-canvas-detail = Plays the short video an artist made for a song in place of its cover
settings-video = Video
settings-video-detail = Shows the video of a YouTube track beside its sound
settings-video-quality = Maximum video quality
settings-video-quality-detail = The sharpest picture fetched, a lower one saves data
settings-pip-corner = Picture-in-picture corner
settings-pip-corner-detail = Where the floating video window opens
settings-canvas-codec = Video decoder
settings-canvas-codec-ready = OpenH264 Video Codec provided by Cisco Systems, Inc.
settings-canvas-codec-missing = Needed to play a canvas or a video, downloaded from Cisco
settings-canvas-codec-failed = The download did not work, try again
settings-canvas-codec-installing = Downloading…
settings-canvas-codec-fetch = Download
settings-canvas-codec-remove = Remove
settings-visualizer = Style
settings-visualizer-detail = How the spectrum is drawn, or whether it is drawn at all
settings-visualizer-style-none = Off
settings-visualizer-style-bars = Bars
settings-visualizer-style-wave = Wave
settings-visualizer-style-both = Bars and wave
settings-visualizer-style-mirror = Mirrored bars
settings-visualizer-style-dots = Dots
settings-visualizer-style-line = Line only
settings-visualizer-color-accent = Accent
settings-visualizer-color-cover = Album art
settings-visualizer-color-text = Text
settings-visualizer-color-custom = Custom
settings-group-spectrum-app = Spectrum in the app
settings-group-spectrum-mini = Spectrum in the mini player
settings-spectrum-strip = Above the player bar
settings-spectrum-strip-detail = Show the spectrum as a band standing on the player bar
settings-spectrum-strip-height = Band height
settings-spectrum-strip-height-detail = How tall the band above the player bar stands, against the bar itself
settings-spectrum-fullscreen = Behind fullscreen artwork
settings-spectrum-fullscreen-detail = Show the spectrum rising behind the artwork in fullscreen
settings-spectrum-intensity = Intensity
settings-spectrum-intensity-detail = How far the bands swing. Above 100% the loud ones flatten against the ceiling
settings-spectrum-color = Colour
settings-spectrum-color-detail = Where the spectrum takes its colour from. Set any other one with a hex value in settings.json
settings-spectrum-gradient = Fade to
settings-spectrum-gradient-detail = A second colour the spectrum fades into towards its top
settings-spectrum-gradient-none = None
settings-spectrum-bar-width = Bar width
settings-spectrum-bar-width-detail = Thin bars read as a smooth surface, wide ones as blocks
settings-spectrum-peaks = Peak marks
settings-spectrum-peaks-detail = Keep a mark at each bar's recent peak that falls back slowly
settings-spectrum-stereo = Split stereo
settings-spectrum-stereo-detail = Run the left channel out to the left and the right one to the right, bass in the middle
settings-spectrum-response = Response
settings-spectrum-response-detail = How long the spectrum takes to follow the music. Lower is snappier, higher is smoother
settings-spectrum-opacity = Opacity
settings-spectrum-opacity-detail = How strongly the spectrum shows through what is drawn over it
settings-spectrum-percent = { $value }%
settings-spectrum-pixels = { $value } px
settings-spectrum-millis = { $value } ms
settings-visualizer-absolute = Ignore volume
settings-visualizer-absolute-detail = Draw the spectrum at the track's own level, however loud Sonora plays it
settings-preview-title = Song title
settings-preview-artist = Artist
settings-preview-album = Album
settings-fullscreen-controls-autohide = Hide controls
settings-fullscreen-controls-autohide-detail = Fade out the playback controls in Now playing while the pointer rests
settings-os-fullscreen = Use OS fullscreen
settings-os-fullscreen-detail = Make Now playing fill the whole screen, the way F11 does
settings-icons = Icon pack
settings-icons-detail = Choose the icon set the interface draws from
settings-motion = Reduce motion
settings-motion-detail = Skip interface animations and transitions
settings-pace = Animation speed
settings-pace-detail = How fast interface animations play
settings-saver = Throttle inactive window
settings-saver-detail = Changes apply after restarting Sonora.
settings-corners = Corners
settings-corners-detail = How rounded surfaces and controls are
settings-blur = Blur UI
settings-blur-detail = Frost the menus and floating controls over whatever they cover
settings-blur-window = Blur window
settings-blur-window-detail = Draw the window over a blurred desktop. Needs an opacity below 100%
settings-font = Font size
settings-font-detail = Base text size, everything else scales with it
settings-font-value = { $size } px
settings-startup = Start on
settings-startup-detail = The screen Sonora opens on launch
settings-startup-no-guest = Unavailable in Guest mode
settings-entries = Sidebar entries
settings-entries-detail = The sections listed in the sidebar
settings-entries-pick = Choose entries
settings-language = Language
settings-language-detail = The language Sonora uses across the interface
settings-language-system = System
settings-language-search = Search a language
settings-language-none = No languages found
settings-typeface = Font
settings-typeface-detail = The typeface Sonora uses across the interface
settings-typeface-system = Default
settings-typeface-search = Search a font
settings-typeface-none = No fonts found
settings-server-side-decorations = Server-side decorations
settings-server-side-decorations-detail = Let the compositor draw the title bar, border and shadow
settings-typeface-loading = Loading…
settings-window-controls = Window controls
settings-window-controls-detail = Draw minimise, maximise and close in the title bar
settings-traffic-light-controls = Traffic light controls
settings-traffic-light-controls-detail = Draw minimise, maximise and close as colored dots
settings-window-rounding = Window corners
settings-window-rounding-detail = How rounded the window's own corners are
settings-controls-side = Controls side
settings-controls-side-detail = Which end of the title bar the controls sit on
settings-mini-on-top = Mini player stays on top
settings-mini-on-top-detail = Keep it above other windows, even when Sonora is not the one in front
settings-mini-corner = Mini player corner
settings-mini-corner-detail = Where the mini player opens
mini-close = Close the mini player
mini-corner-top-left = Top left
mini-corner-top-right = Top right
mini-corner-bottom-left = Bottom left
mini-corner-bottom-right = Bottom right
settings-close-to-tray = Keep running in the background
settings-close-to-tray-detail = Keep Sonora running and playing after its window closes
settings-tray-icon = Show in the system tray
settings-tray-icon-detail = Put an icon with playback controls in the system tray
settings-spotify-connect = Show as a Spotify device
settings-spotify-connect-name = Device name
settings-spotify-connect-name-detail = How Sonora is named in the device list of Spotify's apps
settings-spotify-connect-name-sonora = Sonora
settings-spotify-connect-name-both = Sonora (computer name)
settings-spotify-connect-name-computer = Computer name
settings-spotify-connect-name-custom = Custom
settings-spotify-connect-custom = Custom name
settings-spotify-connect-custom-detail = The name to show. Left empty, Sonora and the computer's name are used
settings-spotify-connect-custom-hint = Device name
settings-spotify-connect-detail = List Sonora in the device picker of Spotify's apps, so you can control it from your phone and Spotify shows what you play
settings-discord = Show on Discord
settings-discord-detail = Put the track you are playing on your Discord profile
settings-discord-name = Status name
settings-discord-name-detail = What the status is called after the "Listening to" that your friends see
settings-discord-name-sonora = Sonora
settings-discord-name-provider = Provider
settings-discord-name-music = Music
settings-discord-name-title = Title
settings-discord-name-artist = Artist
settings-discord-name-artist-title = Artist - Title
settings-discord-show-paused = Show when paused
settings-discord-show-paused-detail = Keep the status on your Discord profile while the track is paused
settings-discord-badge = Show the provider badge
settings-discord-badge-detail = Mark the status with a small icon of the service the track came from
settings-discord-anonymous = Hide details
settings-discord-anonymous-detail = Say only that music is playing, without the title, artist or artwork
settings-discord-hide-on-connect = Hide during Spotify Connect
settings-discord-hide-on-connect-detail = Leave the status to Spotify's own while Sonora plays as a Spotify device or another device plays
settings-discord-buttons = Buttons
settings-discord-buttons-detail = Links under the status for your friends to open, one to the track on its service and one to Sonora
settings-discord-buttons-pick = Choose buttons
settings-discord-preview-listening = Listening to { $name }
settings-discord-preview-music = Music
settings-discord-preview-elapsed = { $time } elapsed
settings-discord-preview-listen-on = Listen on { $provider }
# the Discord status when the track is left out of it
discord-listening = Listening to music
get-sonora = Get Sonora
settings-fullscreen-controls-autohide-automatic = Automatic
settings-fullscreen-controls-autohide-always-hidden = Always hidden
settings-fullscreen-controls-autohide-always-shown = Always shown
settings-normalisation = Normalize loudness
settings-normalisation-detail = Keeps tracks at a consistent volume
settings-gapless = Gapless playback
settings-gapless-detail = Runs one track into the next without a pause, the way an album was sequenced
settings-keep-tracks = Keep played tracks
settings-keep-tracks-detail = Saves what you play, locked to this computer, so playing it again downloads nothing. Turning this off deletes it and leaves your downloads alone. Spotify follows a change the next time Sonora starts
settings-cache-size = Cache size
settings-cache-size-detail = How much space the tracks you played may take. The ones played longest ago make room first, and downloads do not count toward it
settings-cache-size-value = { $count } GB
settings-cache-used = Space used
settings-cache-used-detail = What the tracks kept after playing take on this computer. Clearing them leaves your downloads alone
settings-cache-clear = Clear
settings-downloads-used = Space used
settings-downloads-used-detail = What the songs you downloaded take on this computer
settings-offline-mode = Offline mode
settings-offline-mode-detail = Show and play only what you downloaded
settings-downloads-remove = Remove all
settings-size-gb = { $size } GB
settings-size-mb = { $size } MB
settings-size-measuring = Measuring…
settings-stream-quality = Streaming quality
settings-stream-quality-detail = Higher levels sound better and use more data. When your plan or a track has less, the next level down plays
settings-stream-quality-hi-res = Hi-res
settings-stream-quality-lossless = Lossless
settings-stream-quality-high = High
settings-stream-quality-normal = Normal
settings-stream-quality-low = Low
settings-stream-quality-flac = FLAC, { $depth }-bit { $rate } kHz
settings-stream-quality-flac-up-to = FLAC, up to { $depth }-bit { $rate } kHz
settings-stream-quality-lossy = { $codec }, { $kbps } kbps
settings-stream-quality-lossy-up-to = { $codec }, up to { $kbps } kbps
settings-stream-format = Stream format
settings-stream-format-detail = What the server converts each track to before sending it
settings-stream-format-original = Original
settings-stream-format-mp3 = MP3
settings-stream-format-opus = Opus
settings-stream-format-aac = AAC
settings-stream-bitrate = Maximum bitrate
settings-stream-bitrate-detail = The server converts anything higher down to this rate
settings-stream-bitrate-unlimited = No limit
settings-stream-bitrate-value = { $kbps } kbps
settings-stream-bitrate-hourly = About { $megabytes } MB an hour
settings-stream-bitrate-unlimited-detail = Each file at its own bitrate
settings-output-device = Output device
settings-output-device-detail = Audio device where Sonora plays
settings-output-device-system = System default
settings-output-device-missing = Saved device (not connected)
settings-output-device-direct = Direct to hardware
settings-sample-rate = Sample rate
settings-sample-rate-detail = What the device runs at. Sonora converts every track to it
settings-sample-rate-value = { $khz } kHz
settings-sample-rate-change = Change
settings-buffer = Buffer size
settings-buffer-detail = How much audio Sonora hands the device at a time
settings-output-idle = Not playing
settings-output-device-loading = Loading devices…
settings-output-failed = Device not supported
settings-output-failed-detail = Error: { $error }
settings-signal-source = Source
settings-signal-normalized = Loudness
settings-signal-gain = { $db } dB
settings-signal-resampled = Resampling
settings-signal-khz = { $khz } kHz
settings-signal-kbps = { $kbps } kbps
settings-signal-bits = { $depth }-bit
settings-signal-source-detail = { $rate } · { $quality }
settings-sleep = Sleep timer
settings-sleep-detail = Lets the music stop on its own after a set time, so it can play you to sleep
settings-stay-awake = Stay awake while playing
settings-stay-awake-detail = Prevent sleep while playing and keep the display on while Now playing is open
settings-sleep-configure = Configure…
settings-sleep-off = Off
settings-sleep-end-of-track = End of track
settings-sleep-minutes = { $count } mins
settings-widevine = Widevine module
settings-widevine-detail = Apple Music tracks are encrypted and need Google's Widevine module. Sonora uses one it downloaded from Google with your consent, or else the copy a browser here already has.
settings-widevine-none = Apple Music tracks are encrypted and need Google's Widevine module. No browser here has one, so Sonora can download it from Google with your consent.
settings-widevine-looking = Looking…
settings-widevine-asking = Waiting for your answer
settings-widevine-fetching = Downloading…
settings-widevine-installing = Installing…
settings-widevine-installed = Found in a browser
settings-widevine-fetched = Downloaded from Google
settings-widevine-missing = Not installed
settings-widevine-configured = Set by the environment
settings-widevine-fetch = Download
settings-widevine-uninstall = Uninstall
confirm-uninstall-widevine-title = Uninstall the Widevine module?
confirm-uninstall-widevine = Sonora deletes the copy it downloaded from Google. Apple Music tracks stop playing until it is downloaded again.
confirm-remove-downloads-title = Remove all downloads?
confirm-remove-downloads = Every song you downloaded is deleted from this computer, from every service. They stream again until you download them once more.
widevine-prompt-title = Widevine module
widevine-prompt-wanted = Apple Music tracks are encrypted and need Google's Widevine module. No browser on this computer has one. Sonora can download it from Google's servers, the same file Chrome installs, and keep it in its own folder.
widevine-prompt-replace = Apple Music tracks are encrypted and need Google's Widevine module. A browser on this computer has one, and Sonora can download Google's own copy to use instead, the same file Chrome installs, kept in its own folder.
widevine-prompt-downloading = Downloading…
widevine-prompt-terms = Version { $version } is downloaded. Installing it means accepting Google's terms for the module:
widevine-prompt-installing = Installing…
widevine-prompt-download = Download
widevine-prompt-later = Not now
widevine-prompt-accept = Accept and install
widevine-prompt-decline = Decline
canvas-prompt-title = Play this song's canvas?
canvas-prompt-detail = A canvas is the short video an artist made for a song. Playing it needs the OpenH264 decoder, which Sonora downloads from Cisco once.
canvas-prompt-download = Download
canvas-prompt-later = Not now
canvas-prompt-installing = Downloading the decoder…
settings-equalizer = Equalizer
settings-equalizer-detail = Shapes the sound with a preset or a curve of your own
settings-equalizer-preset-detail = A ready-made sound, or one of your own profiles
settings-equalizer-custom = Custom
settings-equalizer-flat = Flat
settings-equalizer-bass-boost = Bass boost
settings-equalizer-bass-reducer = Bass reducer
settings-equalizer-treble-boost = Treble boost
settings-equalizer-vocal = Vocal
settings-equalizer-rock = Rock
settings-equalizer-pop = Pop
settings-equalizer-jazz = Jazz
settings-equalizer-classical = Classical
settings-equalizer-electronic = Electronic
settings-equalizer-acoustic = Acoustic
settings-equalizer-loudness = Loudness
equalizer-advanced = Advanced
equalizer-simple = Simple
equalizer-sound = Sound
equalizer-profile-default = Default
equalizer-profile-new = Profile { $number }
equalizer-profile-pasted = Pasted profile
equalizer-profile-name-hint = Profile name
equalizer-save-as = Save as profile
equalizer-rename = Rename profile
equalizer-delete = Delete profile
equalizer-delete-confirm = Delete “{ $name }”? This cannot be undone.
equalizer-undo = Undo
equalizer-redo = Redo
equalizer-bass = Bass
equalizer-treble = Treble
equalizer-tone-reset = Reset to 0 dB
equalizer-preamp = Preamp
equalizer-preamp-auto = Automatic
equalizer-preamp-manual = Manual
equalizer-decibels = dB
equalizer-decibels-value = { $db } dB
equalizer-hertz = Hz
equalizer-q = Q
equalizer-octaves = oct
equalizer-no-gain = —
equalizer-filters = Filters
equalizer-no-filters = No filters yet. Add one, pick a preset above, or import a profile.
equalizer-add-filter = Add filter
equalizer-remove-filter = Remove filter
equalizer-kind-peak = Peak
equalizer-kind-low-shelf = Low shelf
equalizer-kind-high-shelf = High shelf
equalizer-kind-low-pass = Low pass
equalizer-kind-high-pass = High pass
equalizer-kind-notch = Notch
equalizer-kind-band-pass = Band pass
equalizer-kind-all-pass = All pass
equalizer-transfer = Import and export
equalizer-import = Import
equalizer-export = Export
equalizer-import-file = Import file…
equalizer-paste = Paste
equalizer-copy = Copy
equalizer-export-file = Export file…
equalizer-imported = Imported { $name }
equalizer-imported-skipped = Imported { $name } without { $count ->
        [one] one line
       *[other] { $count } lines
    }
equalizer-import-nothing = Found no equalizer settings to import
equalizer-import-too-many = Not imported: { $count ->
        [one] one filter is on
       *[other] { $count } filters are on
    }, more than a profile holds
equalizer-import-too-large = Not imported: the text is over 1 MB, too large for an equalizer profile
equalizer-filter-limit =A profile holds up to { $max } filters.
equalizer-paste-empty = The clipboard holds no text
equalizer-copied = Copied the profile as Equalizer APO text
equalizer-exported = Exported the profile
equalizer-dialog-failed = Could not open the file dialog
equalizer-file-unreadable = Could not read the file
equalizer-file-unwritable = Could not write the file
equalizer-value-refused = Kept the previous value
equalizer-skipped-line = Line { $line }: { $reason }
equalizer-skipped-more = and { $count ->
        [one] one more
       *[other] { $count } more
    }
equalizer-skip-unknown-type = unknown filter type { $name }
equalizer-skip-unreadable = not a filter Sonora can read
equalizer-skip-no-frequency = no frequency
equalizer-skip-no-gain = no gain
equalizer-skip-no-width = no Q or bandwidth
equalizer-skip-expression = a value written as an expression
equalizer-skip-no-preamp = the preamp is not a number
equalizer-skip-unsupported = { $name } is not supported
equalizer-rules = Device rules
equalizer-rule-other = Any other device
equalizer-rule-playing = Playing now
equalizer-rule-disconnected = Not connected
equalizer-rule-replaces = Replaces its rule
equalizer-rule-choose-device = Choose a device
equalizer-rule-choose-sound = Choose a sound
equalizer-rule-add = Add rule
equalizer-rule-remove = Remove rule
equalizer-output-port = { $device } · { $port }
signal-path-codec-alac = ALAC
signal-path-codec-pcm = PCM
signal-path-codec-other = Other codec
equalizer-invalid-frequency = Frequency must be between { $low } and { $high } Hz
equalizer-invalid-gain = Gain must be between { $low } and { $high } dB
equalizer-invalid-q = Q must be between { $low } and { $high }
equalizer-invalid-octaves = Bandwidth must be between { $low } and { $high } octaves
equalizer-invalid-shelf-octaves = A shelf takes a Q, not a bandwidth
equalizer-invalid-preamp = Preamp must be between { $low } and { $high } dB
equalizer-overload = Clipping by { $db } dB
equalizer-overload-slight = Clipping by under 0.1 dB
equalizer-graph-preamp = { $db } dB
equalizer-lower-preamp = Lower preamp
equalizer-level = Level
equalizer-status-bit-perfect = Bit-perfect
equalizer-status-bit-perfect-detail = Bit-perfect from Sonora
equalizer-status-server = The sound server then converts it to { $khz } kHz
equalizer-status-altered = Not bit-perfect due to:
equalizer-status-separator = {", "}
equalizer-status-more = +{ $count }
equalizer-status-equalized = EQ
equalizer-status-volume = Volume
equalizer-status-normalized = Normalized
equalizer-status-rate = { $from } → { $to } kHz
equalizer-status-bits = { $from } → { $to } bit
equalizer-status-float = Float → { $to } bit
equalizer-status-channels = { $from ->
    [1] Mono
    [2] Stereo
   *[other] { $from } ch
} → { $to ->
    [1] mono
    [2] stereo
   *[other] { $to } ch
}
settings-panel-lyrics-size = Text size in the panel
settings-panel-lyrics-size-detail = Size of the lyrics text in the side panel, on top of the base font size
settings-fullscreen-lyrics-size = Text size in Now playing
settings-fullscreen-lyrics-size-detail = Size of the lyrics text in Now playing, on top of the base font size
settings-lyrics-size-value = { $size }%
settings-lyrics-for-local-files = Look up lyrics for local files
settings-lyrics-for-local-files-detail = Use metadata from local files to fetch lyrics from the internet
settings-artwork-for-local-files = Artwork for local files
settings-artwork-for-local-files-detail = Use metadata from local files to find a cover on Deezer for your Discord status
settings-karaoke-lyrics = Karaoke
settings-karaoke-lyrics-detail = Highlight lyrics word by word when timing is available
settings-blur-lyrics = Blur inactive lines
settings-blur-lyrics-detail = Blur upcoming and previous lines in the lyrics panel
settings-lyrics-sample-first = Arbitrary data
settings-lyrics-sample-second = Insert funny text right here
settings-lyrics-sample-japanese = ここに面白い文章を入れてください
settings-lyrics-sample-romanized = koko ni omoshiroi bunshou o irete kudasai
settings-lyrics-sample-fourth = I hope this Japanese checks out
settings-lyrics-sample-fifth = The fifth line
settings-romanized-lyrics = Romanized lyrics
settings-romanized-lyrics-detail = Show locally generated pronunciation for selected writing systems
settings-romanization-writing-systems = Writing systems
settings-lyrics-priority = Provider priority
settings-lyrics-priority-detail = Among lyrics of the same kind, Sonora shows the source higher up
settings-lyrics-providers-detail = Choose which services to search for lyrics
settings-lyrics-providers-selected = { $count ->
    [one] { $count } selected
   *[other] { $count } selected
    }
settings-lyrics-provider-local = Local files
settings-lyrics-provider-subsonic = Subsonic
settings-lyrics-provider-spotify = Spotify
settings-lyrics-provider-youtube = YouTube Music
settings-lyrics-provider-apple-music = Apple Music
settings-lyrics-provider-musixmatch = Musixmatch
settings-lyrics-provider-lrclib = LRCLIB
settings-lyrics-provider-kugou = Kugou
settings-lyrics-provider-netease = NetEase
settings-romanization-japanese = Japanese
settings-romanization-chinese = Chinese
settings-romanization-korean = Korean
settings-romanization-cyrillic = Cyrillic
settings-romanization-greek = Greek
settings-romanization-arabic = Arabic
settings-romanization-other = Other writing systems
settings-group-window = Window
settings-group-text = Text
settings-group-motion = Motion
settings-group-fullscreen = Fullscreen
settings-group-waveform = Waveform
settings-group-title-bar = Title bar
settings-group-window-style = Window style
settings-group-lyrics = Lyrics
settings-group-equalizer = Equalizer
settings-group-discord = Discord
settings-group-project = Project
settings-group-streaming = Streaming
settings-group-spotify-connect = Spotify Connect
settings-adaptive-menu = Adaptive context menu
settings-adaptive-menu-detail = Leaves out entries the row already shows, such as the album or the artist
settings-accounts = Streaming services
settings-accounts-detail = The services this device can play from
settings-provider-none = Not connected
settings-provider-connected = Connected
settings-provider-guest = Playing as a guest
settings-provider-guest-idle = Connected as a guest
settings-provider-switch = Switch to
settings-provider-current = In use
settings-provider-sign-in = Sign in
settings-profile-account = { $provider } · { $account }
settings-sign-out = Sign out
settings-lastfm = Last.fm
settings-lastfm-detail = Sonora scrobbles through your own Last.fm API account. Create one, then paste the key and the secret here.
settings-librefm = Libre.fm
settings-listenbrainz = ListenBrainz
settings-listenbrainz-detail = Paste a user token from your ListenBrainz settings page.
settings-maloja = Maloja
settings-maloja-detail = Point Sonora at your Maloja server and paste one of its API keys.
settings-scrobble-off = Not connected
settings-scrobble-on = Connected
settings-scrobble-waiting = Waiting for your browser…
settings-scrobble-as = Scrobbling as { $name }
settings-scrobble-failed = Could not connect
settings-scrobble-connect = Connect
settings-scrobble-disconnect = Disconnect
settings-scrobble-title = Connect { $service }
settings-scrobble-request = Get an API key
settings-scrobble-token-request = Get a token
settings-scrobble-key = API key
settings-scrobble-secret = API secret
settings-scrobble-token = User token
settings-scrobble-server = Server address
settings-local-folder = Music folders
settings-local-folder-empty = Not configured
settings-choose-folder = Choose folder…
settings-add-folder = Add folder
settings-remove-folder = Remove folder
settings-rescan = Rescan
settings-scan-walking = Scanning…
settings-scan-progress = { $percent }%
settings-scan-done = Done in { $seconds }s
settings-version = Version
settings-version-detail = The build of sonora you are running
settings-license = License
settings-license-detail = GNU General Public License version 3 or later
settings-source = Source code
settings-source-detail = The corresponding source for this build
settings-team = Team
settings-role-lead-maintainer = Lead Maintainer
settings-role-maintainer = Maintainer
settings-role-contributor = Contributor
settings-notice = Copyright © 2026 Sonora Contributors. Sonora comes with absolutely no warranty. It is free software, and you are welcome to redistribute it under the terms of the GNU General Public License version 3 or later. Sonora is unofficial and is not affiliated with Spotify AB.

# themes
theme-system = System
theme-dark = Dark
theme-light = Light
theme-unavailable = { $name } (unavailable)

# corners
corners-square = Square
corners-subtle = Subtle
corners-rounded = Rounded
corners-round = Round

# motion
motion-system = System
motion-always = Always
motion-never = Never
pace-slow = Slow
pace-base = Standard
pace-quick = Quick
saver-off = Off
saver-fps = { $fps } FPS

toast-playlist-created = Playlist created
toast-playlist-renamed = Playlist renamed
toast-playlist-deleted = Playlist deleted
toast-local-delete-failed = Some track files could not be deleted
toast-playlist-added = Playlist added to your library
toast-playlist-removed = Playlist removed from your library
toast-playlist-visibility = Playlist visibility changed
toast-track-added = Added to { $name }
toast-track-removed = Removed from { $name }
toast-track-removed-unnamed = Removed from the playlist
toast-playlist-failed = That change could not be saved
toast-session-expired = Your { $name } session expired. Sign in again to see your library.
toast-playlist-busy = Another change is still running
toast-playlist-signed-out = Sign in to change playlists
toast-queued-track = { $name } added to the queue
toast-next-track = { $name } plays next
toast-last-track = { $name } plays last
toast-queued-album = Album added to the queue
toast-next-album = Album plays next
toast-last-album = Album plays last
toast-queued-playlist = Playlist added to the queue
toast-next-playlist = Playlist plays next
toast-last-playlist = Playlist plays last
toast-queued-artist = Artist added to the queue
toast-next-artist = Artist plays next
toast-last-artist = Artist plays last
toast-queue-failed = That could not be added to the queue
toast-queue-local-elsewhere = Local files cannot be queued on another device
toast-queued-tracks = { $count } tracks added to the queue
toast-next-tracks = { $count } tracks play next
toast-last-tracks = { $count } tracks play last
toast-queue-cleared = Queue cleared
toast-queue-removed-track = Removed { $name } from the queue
toast-queue-removed-tracks = Removed { $count } tracks from the queue
toast-sleep-ended = Sleep timer ended
toast-radio-failed = Could not start radio for { $name }
toast-similar-failed = Could not find tracks similar to { $name }
toast-open-failed = Could not open { $name }
toast-open-unsupported = { $name } is not an audio file Sonora can play
toast-open-all-failed = { $count } files could not be opened
toast-open-some-failed = Some files could not be opened
toast-keys-refused = Spotify is not granting this account playback keys
toast-sign-in-to-play = { $name } only streams to a signed-in listener
toast-restore-failed = { $name } could not sign back in, so Sonora carried on without it
toast-track-unplayable = { $name } could not be played
toast-playback-stopped = Playback stopped after several tracks in a row could not be played
toast-widevine-failed = Playback failed
toast-widevine-borrowed = Try updating Widevine
toast-open-settings = Settings
toast-throttled = { $name } is limiting playback for now. Trying again in a moment
toast-still-throttled = { $name } is still limiting playback. Press play to try again
toast-library-add-failed = { $name } could not be added to your library
toast-library-remove-failed = { $name } could not be removed from your library
toast-library-added = Added to your library
toast-library-removed = Removed from your library
toast-library-removed-one = Removed { $name } from your library
toast-library-songs-removed = Removed { $count ->
        [one] one song
       *[other] { $count } songs
    } from your library
toast-library-albums-removed = Removed { $count ->
        [one] one album
       *[other] { $count } albums
    } from your library
toast-playlists-removed = Removed { $count ->
        [one] one playlist
       *[other] { $count } playlists
    } from your library
toast-favorite-removed = Removed { $name } from your favorites
toast-favorite-songs-removed = Removed { $count ->
        [one] one song
       *[other] { $count } songs
    } from your favorites
toast-favorite-albums-removed = Removed { $count ->
        [one] one album
       *[other] { $count } albums
    } from your favorites
toast-favorite-artists-removed = Removed { $count ->
        [one] one artist
       *[other] { $count } artists
    } from your favorites
toast-tracks-removed = Removed { $count ->
        [one] one song
       *[other] { $count } songs
    } from { $name }
toast-tracks-removed-unnamed = Removed { $count ->
        [one] one song
       *[other] { $count } songs
    } from the playlist
toast-unpinned = Unpinned { $name }

# lyrics
lyrics-title = Lyrics
lyrics-idle = Play something to see its lyrics
lyrics-loading = Looking for lyrics…
lyrics-missing = No lyrics found, sorry!
lyrics-instrumental = This song is instrumental
lyrics-failed = Could not reach the lyrics service
lyrics-follow = Follow the song again
lyrics-source = Lyrics from { $source }
lyrics-source-local = Lyrics from the local file
lyrics-writers = Written by { $writers }

update-available = Sonora { $version } is out
update-detail = You are on { $running }. Read what changed, or update now.
update-detail-notes = You are on { $running }. Read what changed, then update Sonora the way you installed it.
update-notes = What's new
update-now = Update
update-working = Downloading the update…
update-failed = The update could not be installed. Try again from the releases page.
update-never = Don't check for updates
notice-never = Don't show again
requirement-audio-bridge-title = Sonora cannot reach your sound server
requirement-audio-bridge = { $server ->
    [pulseaudio] PulseAudio is running, but ALSA has no route to it, so nothing you play is heard. Install pulseaudio-alsa and restart Sonora.
   *[pipewire] PipeWire is running, but ALSA has no route to it, so nothing you play is heard. Install pipewire-alsa and restart Sonora.
}
requirement-audio-output-title = No sound output
requirement-audio-output = Sonora could not open your audio device, so nothing you play is heard. Check that a device is connected and not held by another app.
requirement-web-engine-title = Browser sign-in is unavailable
requirement-web-engine = Install webkit2gtk-4.1 to sign in to YouTube Music, Deezer or Apple Music in a browser window. Until then, paste your cookies instead.
requirement-help = What to install
settings-check-updates = Check for updates
settings-check-updates-detail = Ask GitHub once at startup whether a newer version is out. Sonora installs the update itself on Windows only; elsewhere it points you at what changed
settings-log = Log file
settings-log-detail = What Sonora wrote while running. Attach it to a bug report
settings-log-open = Open log

# tags
tags-edit-title = Edit tags
tags-sheet-song = Song
tags-sheet-album = Album
tags-sheet-details = Details
tags-title = Title
tags-artist = Artist
tags-artists = Artists
tags-add-artist = Add an artist
tags-remove-artist = Remove this artist
tags-track = Track number
tags-track-total = Tracks on release
tags-disc = Disc number
tags-disc-total = Discs in release
tags-album = Album
tags-album-artist = Album artist
tags-year = Year
tags-genre = Genre
tags-composer = Composer
tags-publisher = Publisher
tags-isrc = ISRC
tags-comment = Comment
toast-tags-saved = Saved the tags for { $name }
toast-tags-failed = The tags could not be saved
toast-output-select-failed = Unable to select { $name }
toast-output-default-failed = Unable to select the system default device

nav-pin = Pin
toast-library-pin-failed = Could not update the pin.
toast-jam-unreadable = That link does not lead to a Jam you can join.
toast-jam-needs-connect = Turn on “Show as a Spotify device” in settings to join a Jam
toast-jam-failed = The Jam could not be started or joined.
toast-jam-ended = Jam ended
toast-jam-left = You left the Jam
toast-jam-leave-failed = The Jam could not be ended or left.
nav-nothing-pinned = Nothing here
nav-pins-alphabetical = Alphabetical
nav-pins-kind = By type
nav-show-full-library = Show full library
nav-return-top = Back to top

# trouble
trouble-offline = No connection
trouble-offline-detail = Check your internet connection and try again.
trouble-not-loaded = Could not load
trouble-retry = Try again
toast-offline = No connection. Nothing will stream until it is back.
# jam
jam-title = Jam
jam-empty = Share what you are playing with the speakers around you
jam-start = Start a jam
jam-stop = Stop the jam
jam-join = Join a jam
jam-join-hint = Address, like 192.168.1.20:8990
jam-code-hint = Code
jam-cancel = Cancel
jam-leave = Leave
jam-kick = Remove this listener
jam-cap-add = Add tracks
jam-cap-control = Control playback
jam-cap-browse = Browse playlists
jam-cap-favorite = Save favorites
jam-demote-all = Everyone back to listening
jam-open-on = Open on another device
jam-copy = Copy the address
jam-code = Code { $code }
jam-lead = Lead
jam-lead-detail = How far ahead of the sound the stream runs
jam-lead-auto = automatic · { $lead } ms
jam-need = { $need } ms
jam-joining = Connecting to { $at }
jam-lost = Lost the host, trying again
jam-listening = Listening to { $room }
jam-no-listeners = Nobody is listening yet
jam-listeners = { $count ->
    [one] { $count } listener
   *[other] { $count } listeners
}
jam-refused-protocol = That Sonora speaks a different jam version
jam-refused-code = That code does not open this jam
jam-refused-codec = That device cannot play what this jam sends
jam-refused-full = This jam is full
jam-refused-closed = This jam has nothing to send yet
jam-refused-kicked = The host removed this device
toast-jam-refused = { $reason }
toast-jam-copied = Copied the jam address

# jam receiver page
jam-page-title = Sonora jam
jam-page-play = Play
jam-page-stop = Stop
jam-page-offset = Extra delay on this device
jam-page-sync = Clock
jam-page-drift = Drift
jam-page-behind = This device cannot keep up with the host's lead, raise it in Sonora
jam-page-buffer = Buffer
jam-page-waiting = Waiting for the host
jam-page-connected = Connected
jam-page-lost = Lost the host
jam-page-nothing = Nothing playing
jam-page-playing = Playing
jam-page-paused = Paused
jam-page-tap = Tap play to hear this jam
jam-page-silent = Sound is arriving but nothing is scheduled, try play again
jam-page-local = Local files
jam-page-chunks = chunks
jam-page-search = Add a track to the queue
jam-page-search-hint = Search what the host can play
jam-page-search-go = Search
jam-page-searching = Searching
jam-page-no-hits = Nothing found
jam-page-added = Added
jam-page-denied-closed = The host does not take requests
jam-page-denied-unknown = That track cannot be added
jam-page-denied-busy = Too many requests, wait a moment
jam-page-denied-forbidden = The host has not allowed that
jam-page-you = Your name
jam-page-you-hint = What the room calls you
jam-page-room = In the room
jam-page-transport = Playback
jam-page-prev = Previous
jam-page-next = Next
jam-page-resume = Play
jam-page-pause = Pause
jam-page-shuffle = Shuffle
jam-page-repeat = Repeat
jam-page-here = Volume on this device
jam-page-spectrum = Show the spectrum
jam-page-equalizer = Equalizer on this device
jam-page-equalizer-on = Use the equalizer
jam-page-everyone = Volume for everyone
jam-page-queue = Up next
jam-page-queue-empty = Nothing queued
jam-page-now = Playing now
jam-page-played = Played
jam-page-drop = Remove
jam-page-more = More tracks follow than fit here
jam-page-alone = Nobody else is listening
jam-page-library = The host's playlists
jam-page-library-empty = No playlists to show
jam-page-back = Back
jam-page-play-next = Play next
jam-page-add-all = Add to the queue
jam-page-opening = Opening
jam-page-lyrics = Lyrics
jam-page-lyrics-none = No lyrics for this track
jam-page-favorite = Save to favorites
jam-page-favorited = Saved to favorites

# jam settings
settings-group-jam = Jam
settings-jam-name = Jam name
settings-jam-name-detail = What listeners see when they join
settings-jam-guests = Listeners can add tracks
settings-jam-guests-detail = Anyone in a jam can search your providers and add to the queue
toast-offline-unsupported = { $name } plays downloads only when online.
toast-offline-mode = You are offline. Go online to stream this song.
offline-title = You are offline.
offline-go-online = Go online
toast-download-failed = Could not list the tracks to download.
toast-downloading = Downloading { $name }
toast-downloading-songs = { $count ->
    [one] Downloading { $count } track
   *[other] Downloading { $count } tracks
}
toast-downloaded = Downloaded { $name }
toast-downloaded-songs = { $count ->
    [one] Downloaded { $count } track
   *[other] Downloaded { $count } tracks
}
toast-download-set-failed = Some tracks of { $name } could not be downloaded
toast-download-song-failed = { $name } could not be downloaded
toast-download-songs-failed = Some of the tracks could not be downloaded
toast-percent = { $percent }%
toast-download-cancelled = Cancelled downloading { $name }
toast-downloads-cancelled = { $count ->
    [one] Cancelled downloading { $count } track
   *[other] Cancelled downloading { $count } tracks
}
toast-settings-broken = Fix line { $name } of settings.json to save changes
toast-tray-unavailable = There is no system tray to put the icon in
toast-undo = Undo
toast-link-copied = Link copied
toast-link-sign-in = Sign in to { $name } to open this link
toast-link-unreadable = That link does not lead anywhere Sonora can open
toast-code-copied = Code copied
menu-copy-link-none = This has no link to share
menu-copy-link-local = Local artists and playlists have no link
toast-settings-save-failed = Settings could not be saved
toast-settings-overwritten = settings.json changed on disk, so your last change was not saved
toast-widevine-download-failed = Widevine could not be downloaded
toast-widevine-install-failed = Widevine could not be installed
toast-widevine-remove-failed = Widevine could not be removed
toast-library-scanned = { $count ->
    [one] Library scanned, one song found
   *[other] Library scanned, { $count } songs found
}
toast-scan-failed = Your music folders could not be scanned
toast-folder-removed = Removed { $name }
toast-folder-missing = { $name } is no longer on disk
toast-history-removed = { $count ->
    [one] Removed from history
   *[other] Removed { $count } songs from history
}
toast-history-remove-failed = A song could not be removed from history
toast-history-restore-failed = History could not be restored
toast-history-clear-failed = History could not be cleared

# power
wake-reason = Music is playing

# mobile (iPhone app only, the desktop client never reads these)
mobile-tab-home = Home
mobile-tab-search = Search
mobile-tab-library = Library
mobile-home-empty = Nothing here yet
mobile-home-listen-again = Listen again
mobile-home-quick-picks = Quick picks
mobile-search-placeholder = Songs, albums, artists
mobile-search-empty = Nothing found
mobile-search-tracks = Songs
mobile-search-albums = Albums
mobile-search-artists = Artists
mobile-search-playlists = Playlists
mobile-library-tracks = Songs
mobile-library-albums = Albums
mobile-library-artists = Artists
mobile-library-playlists = Playlists
mobile-library-empty = Your library is empty
mobile-library-sign-in = Sign in to a service under Accounts to see your library
mobile-artist-top-tracks = Top songs
mobile-artist-albums = Albums
mobile-accounts = Accounts
mobile-account-connecting = Connecting…
mobile-account-expired = Sign in again
mobile-account-failed = Failed: { $message }
mobile-account-guest = Guest
mobile-account-signed-in = Signed in as { $account }
mobile-account-signed-out = Signed out
mobile-sign-out = Sign out
mobile-sign-out-title = Sign out of { $provider }?
mobile-sign-out-detail = Its songs leave your queue, and you sign in again to play them.
mobile-prompt-url = Continue in the browser to sign in.
mobile-prompt-url-paste = If the browser could not get back to Sonora, paste the address it ended up on.
mobile-prompt-url-placeholder = http://127.0.0.1:8989/login?code=…
mobile-prompt-web = Sign in on the page that opened. It closes by itself once you are in.
mobile-prompt-code = Enter this code on the page that opens.
mobile-prompt-open = Open
mobile-prompt-secret = Paste the value the sign-in asks for.
mobile-prompt-secret-placeholder = Paste here
mobile-prompt-submit = Submit
mobile-prompt-accounts = Choose an account.
mobile-play = Play
mobile-pause = Pause
mobile-next = Next
mobile-previous = Previous
mobile-shuffle = Shuffle
mobile-repeat = Repeat
mobile-output-playing-on = Playing on { $device }
mobile-open-player = Open player
mobile-nothing-playing = Nothing playing
mobile-queue = Queue
mobile-queue-empty = Your queue is empty
mobile-play-next = Play next
mobile-add-to-queue = Add to queue
mobile-remove = Remove from queue
mobile-save = Save
mobile-unsave = Remove from library
mobile-explicit = Explicit
mobile-back = Back
mobile-close = Close
mobile-dismiss = Dismiss
mobile-cancel = Cancel
mobile-settings = Settings
mobile-settings-accounts = Accounts
mobile-settings-accounts-detail = Spotify, YouTube Music and SoundCloud
mobile-settings-intro = Show the intro again
mobile-settings-about = About
mobile-settings-appearance = Appearance
mobile-settings-accent = Accent colour
mobile-accent-orange = Orange
mobile-accent-red = Red
mobile-accent-pink = Pink
mobile-accent-purple = Purple
mobile-accent-blue = Blue
mobile-accent-teal = Teal
mobile-accent-green = Green
mobile-accent-yellow = Yellow
mobile-settings-background = Background
mobile-settings-background-dark = Dark
mobile-settings-background-black = Black
mobile-settings-background-dim = Dim
mobile-settings-corners = Corners
mobile-settings-corners-sharp = Sharp
mobile-settings-corners-round = Round
mobile-settings-default = Default
mobile-settings-text-size = Text size
mobile-settings-text-size-small = Small
mobile-settings-text-size-large = Large
mobile-settings-artwork = Artwork in the player
mobile-settings-artwork-square = Square
mobile-settings-artwork-rounded = Rounded
mobile-settings-artwork-circle = Circle
mobile-settings-reduce-motion = Reduce motion
mobile-settings-reduce-motion-detail = Keeps the intro and other decorative animations still
mobile-settings-reduce-motion-system = On, because Reduce Motion is on in the iPhone's settings
mobile-settings-content = Playback and content
mobile-settings-hide-explicit = Hide explicit songs
mobile-settings-hide-explicit-detail = Leaves them out of Home, Search, your library and every album, artist and playlist
mobile-settings-badges = Show the service on rows
mobile-settings-badges-detail = Names the service each song, album, artist and playlist comes from
mobile-settings-home = Home and library
mobile-settings-home-shelves = Shelves on Home
mobile-settings-library-tab = Library opens on
mobile-settings-advanced = Advanced
mobile-settings-reset = Reset all settings
mobile-settings-reset-title = Reset all settings?
mobile-settings-reset-detail = Every setting goes back to how it was at first. Your accounts and your library stay as they are.
mobile-settings-reset-confirm = Reset
mobile-build-version = Version { $version }
mobile-build-number = Build { $number }
mobile-build-dev = Development build
mobile-intro-skip = Skip
mobile-intro-welcome-title = Welcome to Sonora
mobile-intro-welcome-body = All your music in one place. Spotify, YouTube Music and SoundCloud, side by side.
mobile-intro-start = Get started
mobile-intro-connect-title = Bring your music
mobile-intro-connect-body = Sign in to the services you use. You can always add more later under Settings.
mobile-intro-continue = Continue
mobile-intro-later = Maybe later
mobile-intro-done-title = You're all set
mobile-intro-done-body = { $count ->
    [0] Nothing connected yet. Sign in under Settings whenever you like.
    [one] One service is connected. Enjoy the music.
   *[other] { $count } services are connected. Enjoy the music.
}
mobile-intro-finish = Start listening
toast-sample-plain = Event has been created
toast-sample-done = Saved to your library
toast-sample-info = Arrive 10 minutes before the event.
toast-sample-warning = Your session ends in five minutes
toast-sample-failed = The server could not be reached
toast-sample-loading = Uploading your tracks
toast-sample-event = Event created
toast-sample-event-detail = Sunday, December 3 at 9:00 AM
toast-sample-delete = Deleted { $name }
toast-sample-undo = Undo
toast-sample-cancel = Cancel
toast-sample-undone = Undone
toast-sample-queued = { $name } added to the queue
toast-sample-long = This toast says a good deal more than the others, so the text has to wrap onto a second and maybe a third line without pushing the close button out of the card
toast-sample-saving = Saving { $name }
toast-sample-saved = Saved { $name }
toast-sample-save-failed = { $name } could not be saved
toast-sample-save-failed-detail = The server answered 503 Service Unavailable
toast-sample-lasting = This one stays until you close it
toast-sample-burst = Toast number { $name }

# Frequency response graph
response-graph-hz = { $value } Hz
response-graph-khz = { $value } kHz
response-graph-db = { $value } dB
response-graph-readout = { $frequency } · { $direction ->
        [up] +{ $gain }
        [down] -{ $gain }
       *[flat] { $gain }
    } dB
