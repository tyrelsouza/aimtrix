<!--
  Dev-only showcase of src/styles/aim-chrome.css. Static mock data, no Matrix.
  Open via the "Style demo" link on the main page, or http://localhost:1420/demo
  in a browser while `npm run dev` is running.
-->
<script lang="ts">
  import { DEFAULT_CHAT_ICON, DEFAULT_DM_ICON } from "$lib/icons";
  const icon = (color: string) =>
    "data:image/svg+xml," +
    encodeURIComponent(
      `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="${color}"/></svg>`,
    );
  // Original app glyph: a yellow speech bubble.
  const appIcon =
    "data:image/svg+xml," +
    encodeURIComponent(
      `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" shape-rendering="crispEdges"><path d="M2 2h12v8H7l-3 3v-3H2z" fill="#ffd400" stroke="#000"/></svg>`,
    );
  const alice = icon("#dc3c3c");
  const bob = icon("#3c5adc");
  const chat = icon("#3caa5a");
  const none = DEFAULT_DM_ICON;
</script>

<div class="demo">
  <!-- Sign On -->
  <section class="aim-window" style="width: 220px">
    <div class="aim-titlebar">
      <img class="aim-titlebar__icon" src={appIcon} alt="" />
      <span class="aim-titlebar__title">Sign On</span>
      <div class="aim-titlebar__controls">
        <button class="aim-titlebar__btn aim-titlebar__btn--minimize" aria-label="Minimize"></button>
        <button class="aim-titlebar__btn aim-titlebar__btn--close" aria-label="Close"></button>
      </div>
    </div>
    <div class="aim-window__body">
      <fieldset class="aim-groupbox">
        <legend>Matrix account</legend>
        <div class="stack">
          <label class="aim-label">Homeserver<input class="aim-field" value="http://localhost:8008" /></label>
          <label class="aim-label">Screen Name<input class="aim-field" value="alice" /></label>
          <label class="aim-label">Password<input class="aim-field" type="password" value="aimtrix-test" /></label>
        </div>
      </fieldset>
      <p class="aim-alert" role="alert">Incorrect screen name or password.</p>
      <div class="row end">
        <button class="aim-btn">Setup</button>
        <button class="aim-btn is-default">Sign On</button>
      </div>
    </div>
  </section>

  <!-- Buddy List -->
  <section class="aim-window" style="width: 220px; height: 420px">
    <div class="aim-titlebar">
      <img class="aim-titlebar__icon" src={appIcon} alt="" />
      <span class="aim-titlebar__title">Alice's Buddy List</span>
      <div class="aim-titlebar__controls">
        <button class="aim-titlebar__btn aim-titlebar__btn--minimize" aria-label="Minimize"></button>
        <button class="aim-titlebar__btn aim-titlebar__btn--maximize" aria-label="Maximize"></button>
        <button class="aim-titlebar__btn aim-titlebar__btn--close" aria-label="Close"></button>
      </div>
    </div>
    <nav class="aim-menubar">
      <button class="aim-menubar__item">My AIMtrix</button>
      <button class="aim-menubar__item">People</button>
      <button class="aim-menubar__item">Help</button>
    </nav>
    <div class="aim-window__body">
      <div class="aim-panel">
        <details class="aim-group" open>
          <summary>Favorites <span class="aim-group__count">(1/1)</span></summary>
          <ul class="aim-list">
            <li><button class="aim-list__item"><img class="aim-list__icon" src={chat} alt="" />Test Chat</button></li>
          </ul>
        </details>
        <details class="aim-group" open>
          <summary>Buddies <span class="aim-group__count">(2/3)</span></summary>
          <ul class="aim-list">
            <li><button class="aim-list__item is-selected is-unread"><img class="aim-list__icon" src={bob} alt="" />Bob Brown</button></li>
            <li><button class="aim-list__item is-away"><img class="aim-list__icon" src={none} alt="" />Carol Chen</button></li>
            <li><button class="aim-list__item is-offline"><img class="aim-list__icon" src={none} alt="" />Dave (offline)</button></li>
          </ul>
        </details>
        <details class="aim-group" open>
          <summary>Chats <span class="aim-group__count">(1)</span></summary>
          <ul class="aim-list">
            <li><button class="aim-list__item is-unread"><img class="aim-list__icon" src={DEFAULT_CHAT_ICON} alt="" />Secret Chat</button></li>
          </ul>
        </details>
        <details class="aim-group" open>
          <summary>Invitations <span class="aim-group__count">(1)</span></summary>
          <ul class="aim-list">
            <li>
              <button class="aim-list__item"><img class="aim-list__icon" src={DEFAULT_CHAT_ICON} alt="" />Invite Test</button>
              <div class="row" style="gap: 4px; padding: 1px 0 3px 38px">
                <button class="aim-btn aim-btn--small">Accept</button>
                <button class="aim-btn aim-btn--small">Decline</button>
              </div>
            </li>
          </ul>
        </details>
      </div>
      <div class="row center">
        <button class="aim-btn" style="min-width: 0">IM</button>
        <button class="aim-btn" style="min-width: 0">Chat</button>
        <button class="aim-btn" style="min-width: 0" disabled>Info</button>
      </div>
    </div>
    <div class="aim-statusbar">
      <span class="aim-statusbar__section">Online</span>
      <span class="aim-statusbar__section aim-statusbar__section--fit">localhost</span>
    </div>
  </section>

  <!-- IM window -->
  <section class="aim-window" style="width: 360px; height: 340px">
    <div class="aim-titlebar">
      <img class="aim-titlebar__icon" src={appIcon} alt="" />
      <span class="aim-titlebar__title">Bob Brown - Instant Message</span>
      <div class="aim-titlebar__controls">
        <button class="aim-titlebar__btn aim-titlebar__btn--minimize" aria-label="Minimize"></button>
        <button class="aim-titlebar__btn aim-titlebar__btn--maximize" aria-label="Maximize"></button>
        <button class="aim-titlebar__btn aim-titlebar__btn--close" aria-label="Close"></button>
      </div>
    </div>
    <div class="aim-window__body">
      <div class="row grow">
        <div class="aim-panel aim-transcript">
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--self">Alice Anderson:</span> hey bob, you there?</p>
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> yep! what's up</p>
          <p class="aim-msg"><span class="aim-msg__time">(9:41 PM)</span><span class="aim-msg__name aim-msg__name--self">Alice Anderson:</span> testing AIMtrix</p>
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--self">Alice Anderson:</span> did this send?<span class="aim-msg__note"> (not sent)</span></p>
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> <span class="aim-msg__html">nice, this is <b>bold</b> and <span style="color: #ff0000">red</span>, a <a href="https://example.org">link</a>, <code>code</code> and a <span data-mx-spoiler="">spoiler</span></span></p>
          <div class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> <span class="aim-msg__html"><blockquote>a quote from another client</blockquote>and the reply</span></div>
          <p class="aim-msg aim-msg--undecryptable"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> 🔒 Unable to decrypt this message yet.</p>
          <div class="aim-msg"><div class="aim-msg__reply">In reply to <b>Alice Anderson</b>: see you at 5?</div><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> sounds good<span class="aim-msg__note"> (edited)</span></div>
          <div class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span><img class="aim-msg__image" src={bob} alt="cat.png" style="width: 80px" /></div>
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> <span class="aim-msg__file">📎 notes.txt<span class="aim-msg__note"> (12 KB)</span></span></p>
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> <span class="aim-msg__note">(message deleted)</span></p>
          <p class="aim-msg aim-msg--system">Bob Brown is away: "grabbing lunch"</p>
        </div>
        <img class="aim-buddy-icon" src={bob} alt="Bob's buddy icon" />
      </div>
      <div class="aim-toolbar">
        <button class="aim-toolbar__btn" aria-pressed="true"><b>B</b></button>
        <button class="aim-toolbar__btn"><i>I</i></button>
        <button class="aim-toolbar__btn"><u>U</u></button>
        <span class="aim-toolbar__sep"></span>
        <div style="position: relative">
          <button class="aim-toolbar__btn" style="color: #ff0000" aria-expanded="true">A</button>
          <div class="aim-palette">
            {#each ["#000000", "#ff0000", "#0000ff", "#008000", "#800080", "#ff8000", "#008080", "#808080"] as hex (hex)}
              <button class="aim-palette__swatch" style:background={hex} aria-label={hex}></button>
            {/each}
          </div>
        </div>
      </div>
      <div class="row">
        <textarea class="aim-field grow" rows="3">see you at 5?</textarea>
        <img class="aim-buddy-icon" src={alice} alt="Your buddy icon" />
      </div>
      <div class="row end">
        <button class="aim-btn">Warn</button>
        <button class="aim-btn">Block</button>
        <button class="aim-btn is-default">Send</button>
      </div>
    </div>
    <div class="aim-statusbar">
      <span class="aim-statusbar__section">Bob Brown is typing…</span>
    </div>
  </section>

  <!-- Chat Room window (inactive title bar) -->
  <section class="aim-window" style="width: 420px; height: 300px">
    <div class="aim-titlebar is-inactive">
      <img class="aim-titlebar__icon" src={appIcon} alt="" />
      <span class="aim-titlebar__title">Test Chat - Chat Room</span>
      <div class="aim-titlebar__controls">
        <button class="aim-titlebar__btn aim-titlebar__btn--minimize" aria-label="Minimize"></button>
        <button class="aim-titlebar__btn aim-titlebar__btn--maximize" aria-label="Maximize"></button>
        <button class="aim-titlebar__btn aim-titlebar__btn--close" aria-label="Close"></button>
      </div>
    </div>
    <div class="aim-window__body">
      <div class="row grow">
        <div class="aim-panel aim-transcript">
          <p class="aim-msg aim-msg--system">Carol Chen has entered the room.</p>
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Carol Chen:</span> hi everyone!</p>
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--self">Alice Anderson:</span> hey carol</p>
          <p class="aim-msg aim-msg--system">Bob Brown has left the room.</p>
        </div>
        <div class="members">
          <span>3 people here</span>
          <ul class="aim-panel aim-list">
            <li class="aim-list__item flush"><img class="aim-list__icon" src={alice} alt="" />Alice Anderson</li>
            <li class="aim-list__item flush"><img class="aim-list__icon" src={bob} alt="" />Bob Brown</li>
            <li class="aim-list__item flush"><img class="aim-list__icon" src={none} alt="" />Carol Chen</li>
          </ul>
        </div>
      </div>
      <div class="row">
        <input class="aim-field grow" placeholder="Say something…" />
        <button class="aim-btn is-default">Send</button>
      </div>
    </div>
  </section>
</div>

<style>
  .demo {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 16px;
    padding: 16px;
    min-height: 100%;
    background: #008080;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    gap: 6px;
    min-height: 0;
  }
  .row.end {
    justify-content: flex-end;
  }
  .row.center {
    justify-content: center;
  }
  .grow {
    flex: 1;
  }
  .members {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 130px;
  }
  .flush {
    padding-left: 4px;
  }
</style>
