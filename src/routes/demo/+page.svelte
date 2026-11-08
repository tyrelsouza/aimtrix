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
          <label class="aim-label">Server<select class="aim-field"><option>matrix.org</option><option selected>Custom…</option></select></label>
          <label class="aim-label">Homeserver<input class="aim-field" value="example.com" /></label>
          <label class="aim-label">Screen Name<input class="aim-field" value="alice" /></label>
          <label class="aim-label">Password<input class="aim-field" type="password" value="aimtrix-test" /></label>
        </div>
      </fieldset>
      <p class="aim-alert" role="alert">Incorrect screen name or password.</p>
      <div class="row end">
        <button class="aim-btn" title="Shown when the server offers OAuth">Use Browser…</button>
        <button class="aim-btn is-default">Sign On</button>
      </div>
    </div>
  </section>

  <!-- About AIMtrix (from Preferences) -->
  <section class="aim-window" style="width: 300px">
    <div class="aim-titlebar">
      <img class="aim-titlebar__icon" src={appIcon} alt="" />
      <span class="aim-titlebar__title">About AIMtrix</span>
      <div class="aim-titlebar__controls">
        <button class="aim-titlebar__btn aim-titlebar__btn--close" aria-label="Close"></button>
      </div>
    </div>
    <div class="aim-window__body">
      <div class="row"><img src={appIcon} alt="" width="48" height="48" /><b>AIMtrix</b> Version 0.0.1</div>
      <p>A Matrix chat client in the style of AOL Instant Messenger. Not affiliated with AOL.</p>
      <p>© 2026 Tyrel Souza. MIT License.</p>
      <div class="row end">
        <button class="aim-btn">License…</button>
        <button class="aim-btn">Third-Party…</button>
        <button class="aim-btn is-default">OK</button>
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
      <!-- .is-open: the menu whose dropdown is showing. -->
      <button class="aim-menubar__item is-open">People</button>
      <button class="aim-menubar__item">Help</button>
    </nav>
    <hr class="aim-sep" />
    <div class="aim-window__body">
      <div class="aim-panel">
        <details class="aim-group" open>
          <summary>Favorites <span class="aim-group__count">(1/1)</span></summary>
          <ul class="aim-list">
            <li><button class="aim-list__item"><img class="aim-list__icon" src={chat} alt="" />Test Chat</button></li>
          </ul>
        </details>
        <details class="aim-group" open>
          <summary>Online <span class="aim-group__count">(2)</span></summary>
          <ul class="aim-list">
            <li><button class="aim-list__item is-selected is-unread"><img class="aim-list__icon" src={bob} alt="" />Bob Brown</button></li>
            <li><button class="aim-list__item is-away" title={"Carol Chen is away:\nAt the dentist, back by 3."}><img class="aim-list__icon" src={none} alt="" />Carol Chen (away)</button></li>
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
        <details class="aim-group" open>
          <summary>Offline <span class="aim-group__count">(2)</span></summary>
          <ul class="aim-list">
            <li><button class="aim-list__item is-offline"><img class="aim-list__icon" src={none} alt="" />Dave</button></li>
            <li><button class="aim-list__item is-offline is-blocked" title="Eve is blocked."><img class="aim-list__icon" src={none} alt="" />Eve</button></li>
          </ul>
        </details>
      </div>
      <div class="row" style="align-items: center; gap: 4px">
        <span class="grow" style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap">You're away: Out to lunch, back soon!</span>
        <button class="aim-btn aim-btn--small">Edit…</button>
        <button class="aim-btn aim-btn--small">I'm Back</button>
      </div>
      <div class="row" style="align-items: center; gap: 4px">
        <span class="grow">🔒 This computer isn't verified.</span>
        <button class="aim-btn aim-btn--small">Verify…</button>
        <button class="aim-btn aim-btn--small">Restore…</button>
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

  <!-- Restore Messages -->
  <section class="aim-window" style="width: 300px">
    <div class="aim-titlebar">
      <img class="aim-titlebar__icon" src={appIcon} alt="" />
      <span class="aim-titlebar__title">Restore Messages</span>
      <div class="aim-titlebar__controls">
        <button class="aim-titlebar__btn aim-titlebar__btn--close" aria-label="Close"></button>
      </div>
    </div>
    <div class="aim-window__body">
      <p style="margin: 0">🔒 This computer can't read your older encrypted messages yet.</p>
      <fieldset class="aim-groupbox">
        <legend>Recovery key</legend>
        <label class="aim-label">
          Enter the recovery key or passphrase you saved when you set up recovery (for example in Element).
          <input class="aim-field" type="password" value="EsTc 1234 abcd" />
        </label>
      </fieldset>
      <p class="aim-alert" role="alert">That recovery key or passphrase isn't right.</p>
      <div class="row end">
        <button class="aim-btn">Not Now</button>
        <button class="aim-btn is-default">Restore</button>
      </div>
    </div>
  </section>

  <!-- Verify This Computer -->
  <section class="aim-window" style="width: 340px">
    <div class="aim-titlebar">
      <img class="aim-titlebar__icon" src={appIcon} alt="" />
      <span class="aim-titlebar__title">Verify This Computer</span>
      <div class="aim-titlebar__controls">
        <button class="aim-titlebar__btn aim-titlebar__btn--close" aria-label="Close"></button>
      </div>
    </div>
    <div class="aim-window__body">
      <p style="margin: 0">Check that these emoji appear in the same order on <b>Firefox on Linux</b>:</p>
      <div class="row center" style="flex-wrap: wrap; gap: 6px 10px; font-size: 11px; text-align: center">
        {#each [["🎁", "Gift"], ["🔨", "Hammer"], ["😀", "Smiley"], ["🐷", "Pig"], ["🍕", "Pizza"], ["🤖", "Robot"], ["🎩", "Hat"]] as [symbol, description]}
          <span style="min-width: 56px"><span style="display: block; font-size: 28px">{symbol}</span>{description}</span>
        {/each}
      </div>
      <div class="row end">
        <button class="aim-btn">They Don't Match</button>
        <button class="aim-btn is-default">They Match</button>
      </div>
    </div>
  </section>

  <!-- Set Up Recovery -->
  <section class="aim-window" style="width: 340px">
    <div class="aim-titlebar">
      <img class="aim-titlebar__icon" src={appIcon} alt="" />
      <span class="aim-titlebar__title">Set Up Recovery</span>
      <div class="aim-titlebar__controls">
        <button class="aim-titlebar__btn aim-titlebar__btn--close" aria-label="Close"></button>
      </div>
    </div>
    <div class="aim-window__body">
      <p style="margin: 0">This is your recovery key. Write it down or save it somewhere safe, like a password manager:</p>
      <code class="aim-field" style="height: auto; padding: 6px; font-family: monospace; text-align: center; word-spacing: 4px">
        EsTd mQ2x 8Kfa Lr3p Vw9z Ny4b Ht7c Jk2s Pq6m Xe5g Wu8f Ra3d
      </code>
      <p style="margin: 0">You'll need it to read your encrypted messages on a new computer. AIMtrix won't show it again.</p>
      <label class="row" style="align-items: center; gap: 4px"><input type="checkbox" /> I've saved my recovery key</label>
      <div class="row end">
        <button class="aim-btn">Copy</button>
        <button class="aim-btn is-default" disabled>Done</button>
      </div>
    </div>
  </section>

  <!-- Away Message -->
  <section class="aim-window" style="width: 300px; height: 220px">
    <div class="aim-titlebar">
      <img class="aim-titlebar__icon" src={appIcon} alt="" />
      <span class="aim-titlebar__title">Away Message</span>
      <div class="aim-titlebar__controls">
        <button class="aim-titlebar__btn aim-titlebar__btn--close" aria-label="Close"></button>
      </div>
    </div>
    <div class="aim-window__body">
      <label class="aim-label grow">
        Away message, shown to buddies while you're away:
        <textarea class="aim-field grow">I am away from my computer right now.</textarea>
      </label>
      <div class="row end">
        <button class="aim-btn">Cancel</button>
        <button class="aim-btn is-default">I'm Away</button>
      </div>
    </div>
  </section>

  <!-- Add Buddy -->
  <section class="aim-window" style="width: 300px">
    <div class="aim-titlebar">
      <img class="aim-titlebar__icon" src={appIcon} alt="" />
      <span class="aim-titlebar__title">Add Buddy</span>
      <div class="aim-titlebar__controls">
        <button class="aim-titlebar__btn aim-titlebar__btn--close" aria-label="Close"></button>
      </div>
    </div>
    <div class="aim-window__body">
      <label class="aim-label">
        Screen name or Matrix ID of the buddy to add:
        <input class="aim-field" value="@dave:localhost" />
      </label>
      <p style="margin: 0; color: var(--c-text-disabled)">They get an invitation, and show up under Buddies.</p>
      <p class="aim-alert" role="alert">There's no one called @dave:localhost.</p>
      <div class="row end">
        <button class="aim-btn">Cancel</button>
        <button class="aim-btn is-default">Add</button>
      </div>
    </div>
  </section>

  <!-- Preferences -->
  <section class="aim-window" style="width: 320px">
    <div class="aim-titlebar">
      <img class="aim-titlebar__icon" src={appIcon} alt="" />
      <span class="aim-titlebar__title">Preferences</span>
      <div class="aim-titlebar__controls">
        <button class="aim-titlebar__btn aim-titlebar__btn--close" aria-label="Close"></button>
      </div>
    </div>
    <div class="aim-window__body">
      <fieldset class="aim-groupbox">
        <legend>Away</legend>
        <label class="row" style="align-items: center; gap: 4px"><input type="checkbox" /> Send my away message as an auto-reply</label>
      </fieldset>
      <fieldset class="aim-groupbox">
        <legend>Tray</legend>
        <label class="row" style="align-items: center; gap: 4px"><input type="checkbox" checked /> Keep running in the tray when the Buddy List is closed</label>
      </fieldset>
      <fieldset class="aim-groupbox">
        <legend>Fonts</legend>
        <div style="display: grid; grid-template-columns: auto 1fr; gap: 4px 6px; align-items: center">
          <span>Conversations:</span>
          <select class="aim-field"><option>Verdana</option></select>
          <span>Size:</span>
          <select class="aim-field"><option>Large</option></select>
          <span>My messages:</span>
          <div class="row" style="gap: 2px">
            {#each ["#000000", "#ff0000", "#0000ff", "#008000"] as hex}
              <span class="aim-palette__swatch" style:background={hex} style:display="inline-block"></span>
            {/each}
          </div>
        </div>
        <p class="aim-field" style="margin: 6px 0 0; font-family: Verdana, var(--font-ui); font-size: 14px; color: #0000ff">Me: Hey, what's up?</p>
      </fieldset>
      <div class="row end">
        <button class="aim-btn">Cancel</button>
        <button class="aim-btn is-default">OK</button>
      </div>
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
      <p class="aim-away-banner"><b>Bob Brown is away</b>: Gone fishing. Leave a message!</p>
      <div class="row grow">
        <div class="aim-panel aim-transcript">
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--self">Alice Anderson:</span> hey bob, you there?</p>
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> yep! what's up</p>
          <p class="aim-msg"><span class="aim-msg__time">(9:41 PM)</span><span class="aim-msg__name aim-msg__name--self">Alice Anderson:</span> testing AIMtrix</p>
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--self">Alice Anderson:</span> did this send?<span class="aim-msg__note"> (not sent)</span></p>
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> <span class="aim-msg__html">nice, this is <b>bold</b> and <span style="color: #ff0000">red</span>, a <a href="https://example.org">link</a>, <code>code</code> and a <span data-mx-spoiler="">spoiler</span></span></p>
          <div class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> <span class="aim-msg__html"><blockquote>a quote from another client</blockquote>and the reply</span></div>
          <p class="aim-msg aim-msg--undecryptable"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> 🔒 (encrypted message, waiting for the key to read it)</p>
          <div class="aim-msg"><div class="aim-msg__reply">In reply to <b>Alice Anderson</b>: see you at 5?</div><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> sounds good<span class="aim-msg__note"> (edited)</span></div>
          <div class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span><button class="aim-msg__attachment" title="Save cat.png…"><img class="aim-msg__image" src={bob} alt="cat.png" style="width: 80px" /></button></div>
          <p class="aim-msg"><span class="aim-msg__name aim-msg__name--buddy">Bob Brown:</span> <button class="aim-msg__attachment" title="Save notes.txt…"><span class="aim-msg__file">📎 notes.txt<span class="aim-msg__note"> (12 KB)</span></span></button></p>
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
        <button class="aim-btn">Send File…</button>
        <button class="aim-btn is-default">Send</button>
      </div>
    </div>
    <div class="aim-statusbar">
      <span class="aim-statusbar__section">Sending beach.jpg (2 of 3)… 45%</span>
      <span class="aim-statusbar__section aim-statusbar__section--fit"><button class="aim-btn aim-btn--small">Cancel</button></span>
    </div>
  </section>

  <!-- Chat Room window (inactive title bar) -->
  <section class="aim-window" style="width: 420px; height: 340px">
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
          <p class="aim-msg aim-msg--system">Carol Chen changed the topic to: weekend plans</p>
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
      <div class="aim-toolbar">
        <button class="aim-toolbar__btn"><b>B</b></button>
        <button class="aim-toolbar__btn"><i>I</i></button>
        <button class="aim-toolbar__btn"><u>U</u></button>
        <span class="aim-toolbar__sep"></span>
        <button class="aim-toolbar__btn">A</button>
      </div>
      <textarea class="aim-field" rows="2">hi all</textarea>
      <div class="row end">
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
