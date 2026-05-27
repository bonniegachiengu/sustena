# Sustena Brand Design Assets — System Document
### For Claude Design + Dispatch Integration
**Owner:** Bonventure Gachiengu
**Version 1.1 — May 2026**
**Cross-reference:** Sustena_XII_Master_Strategy.md §12 | Bonnie_Master_Roadmap.md Appendix A

---

## Purpose

This document is the single source of truth for Sustena's visual brand assets. It defines:
1. Where all brand assets live (folder structure)
2. The canonical design system variables (colours, typography, spacing)
3. Prompt templates for generating each asset type via Claude Design
4. How to reference these assets in Claude Code/Dispatch sessions

**Integration rule:** Before generating any visual asset, check if it already exists in the folder structure below. Before running a Claude Code prompt that references a visual asset (icon, logo, UI component), always specify the asset path from this document — do not regenerate unless the existing asset is wrong.

---

## Part 1 — Folder Structure

All Sustena brand assets live in the following structure. The root is your OneDrive project folder.

```
C:\Users\DELL\OneDrive\Documents\Projects\Sustena XII\Sustena XII\
└── brand/
    ├── design-system/
    │   ├── colours.json          ← canonical colour tokens (machine-readable)
    │   ├── typography.css        ← font stack and scale
    │   └── design-system.md      ← human-readable system spec
    ├── logos/
    │   ├── sustena/
    │   │   ├── sustena-full-dark.svg      ← full wordmark, dark background
    │   │   ├── sustena-full-light.svg     ← full wordmark, light background
    │   │   ├── sustena-icon-only.svg      ← icon only (network node)
    │   │   └── sustena-monochrome.svg     ← monochrome version
    │   ├── vyyb/
    │   │   ├── vyyb-full-dark.svg
    │   │   └── vyyb-icon-only.svg
    │   ├── colosso-finance/
    │   │   ├── colosso-full-dark.svg
    │   │   └── colosso-icon-only.svg
    │   └── 365plus/
    │       └── 365plus-full-dark.svg
    ├── ui-components/
    │   ├── budget-ring/
    │   │   ├── budget-ring-mockup.png
    │   │   └── budget-ring.svg
    │   ├── sustain-card/
    │   │   └── sustain-card-mockup.png
    │   ├── council-proposal/
    │   │   └── council-proposal-card.png
    │   └── orchie-chat/
    │       └── orchie-chat-mockup.png
    ├── pitch-deck/
    │   ├── sustena-pitch-deck-template.pptx
    │   └── slide-backgrounds/
    │       ├── cover-background.png
    │       └── data-slide-background.png
    ├── social/
    │   ├── linkedin-banner.png            ← 1584×396px
    │   ├── twitter-header.png             ← 1500×500px
    │   ├── profile-photo-sustena.png      ← 400×400px logo for social profiles
    │   └── post-templates/
    │       ├── milestone-post.png
    │       └── partnership-post.png
    └── docs/
        ├── favicon.ico
        ├── og-image.png                   ← Open Graph 1200×630 for sustena.io
        └── docs-header.png
```

**When running a Code session:** Reference assets with the `computer://` path prefix or the OneDrive path above. Example: `computer://C:\Users\DELL\OneDrive\Documents\Projects\Sustena XII\Sustena XII\brand\logos\sustena\sustena-icon-only.svg`

---

## Part 2 — The Sustena Design System

> **Source of truth:** `C:\Users\DELL\OneDrive\Documents\Projects\Sustena XI\.claude\worktrees\hungry-ellis-c89ace\ui\static\css\sustena.css`
> This document mirrors that CSS. If there is ever a conflict, the CSS wins. Update this doc to match — never the reverse.

**Design aesthetic:** Industrial-utilitarian. Sustena models reality — the UI should feel like control room instrumentation, not a consumer app. Clean data density, sharp typography, amber/teal accent on near-black surfaces.

### 2.1 Colour Palette

```json
{
  "surfaces": {
    "bg-base":    "#0f0f0f",
    "bg-surface": "#181818",
    "bg-raised":  "#212121",
    "bg-overlay": "#2a2a2a"
  },
  "borders": {
    "border":       "#2e2e2e",
    "border-mid":   "#3a3a3a",
    "border-light": "#4a4a4a"
  },
  "text": {
    "text-primary":   "#e8e4dc",
    "text-secondary": "#8a8680",
    "text-muted":     "#565250",
    "text-dim":       "#3a3836"
  },
  "accent-amber": {
    "amber":        "#E8A020",
    "amber-dim":    "#c07818",
    "amber-glow":   "rgba(232, 160, 32, 0.12)",
    "amber-border": "rgba(232, 160, 32, 0.3)"
  },
  "accent-teal": {
    "teal":         "#2ab8a0",
    "teal-dim":     "#1e8c7a",
    "teal-glow":    "rgba(42, 184, 160, 0.1)",
    "teal-border":  "rgba(42, 184, 160, 0.3)"
  },
  "status": {
    "ok":     "#4caf80",
    "warn":   "#e8a020",
    "danger": "#e05050",
    "info":   "#5090e0"
  }
}
```

**Colour roles:**
- Amber (`#E8A020`) = primary action, live data, active state — "this is real, this is happening"
- Teal (`#2ab8a0`) = simulation mode, secondary interactive elements, save/confirm actions
- Near-black surfaces (`#0f0f0f` → `#2a2a2a`) = the four-step surface stack; use in order, bg-base deepest
- `#e8e4dc` text primary = slightly warm off-white, not pure white — intentional

### 2.2 Typography

**UI font:** Inter Tight (Google Fonts)
- Primary UI chrome, labels, body text
- Weights: 300 (light), 400 (regular), 500 (medium), 600 (semibold)
- Base: 13px / line-height 1.5
- Headings (page-title): 18px / weight 500 / letter-spacing -0.01em

**Monospace / data font:** DM Mono (Google Fonts)
- All data values, technical identifiers, state paths, operator names, timestamps, code, monospace labels
- Weights: 300, 400, 500
- Common sizes: 10px (labels), 11px (metadata), 12px (values), 22px (large metric values)
- Letter-spacing: 0.08–0.12em on uppercase labels

**Import string for HTML/CSS:**
```css
@import url('https://fonts.googleapis.com/css2?family=DM+Mono:wght@300;400;500&family=Inter+Tight:wght@300;400;500;600&display=swap');
```

**Typography rules:**
- Card titles: DM Mono, 11px, uppercase, letter-spacing 0.08em, `--text-secondary`
- Nav labels: Inter Tight, 13px, weight 400
- Metric values: DM Mono, 22px, weight 500
- Metric labels: DM Mono, 10px, uppercase, letter-spacing 0.10em, `--text-muted`
- Status badges: DM Mono, 10px, weight 500, uppercase, letter-spacing 0.08em
- Tab buttons: DM Mono, 11px, uppercase, weight 500, letter-spacing 0.06em

### 2.3 Logo Concept

The Sustena logo is a **network node** — a central point with three branching arcs. It suggests simultaneously:
- A mycelium spore (organic, interconnected, growing)
- A DAO governance node (distributed, network-based)
- A signal transmission point (broadcasting, receiving)

The icon must be geometric and precise — not organic or hand-drawn. It must work at all sizes from 16×16px (favicon) to full-page displays. At small sizes, drop the wordmark and use the icon only.

**Wordmark:** DM Mono weight 500, letter-spacing 0.08em, uppercase. The `S` in `SUSTENA` uses amber (`#E8A020`), remainder uses `--text-primary` (`#e8e4dc`).

### 2.4 UI Component Standards

All components use CSS variables from the design system — never hardcoded hex values in component code (except in the CSS variable definitions themselves).

**Border radius scale:** `--radius-sm: 4px` · `--radius-md: 6px` · `--radius-lg: 10px`

**Transitions:** `--t-fast: 120ms ease` · `--t-mid: 220ms ease` — use only for colour/background/border transitions. No decorative motion.

**Metric card:** `bg-surface` background, `border` 1px, `radius-md`. 2px top-border accent (colour indicates data type: amber = active/live, teal = simulation, ok/green = healthy, danger = alert). Metric label: DM Mono 10px uppercase muted. Metric value: DM Mono 22px weight 500. Flash `.updated` class on value change (turns amber, adds glow shadow).

**Domain/sustain card:** `bg-surface` background, `border` 1px, `radius-md`. Domain header: `bg-raised`, DM Mono 11px uppercase weight 500 `text-secondary`. State rows: `state-key` (DM Mono 11px muted) + `state-val` (DM Mono 12px weight 500 primary). State val modifiers: `.zero` (dim), `.alert` (danger), `.active` (ok).

**Status badge:** DM Mono 10px weight 500 uppercase letter-spacing 0.08em, border-radius 10px. Variants: `badge-amber`, `badge-teal`, `badge-ok`, `badge-danger`, `badge-muted`.

**Event log row:** DM Mono 11px, two-column grid (140px timestamp/type | 1fr detail). Event type: amber. System events: teal. Hover: `bg-raised`.

**Card:** `bg-surface` / `border` / `radius-lg`. Card header: `border-bottom`, `card-title` DM Mono 11px uppercase weight 500 `text-secondary`. Card body: 16px padding.

**Nav link (active):** amber text, `amber-glow` background, 2px amber left border. Inactive: `text-secondary`. Hover: `text-primary` / `bg-raised`. Collapsed state: icon only (16px), centred.

**Council proposal card:** Amber-outlined card (`amber-border`). Header: proposal title in Inter Tight. Body: proposed operator name (DM Mono), expected outcome in plain language, operative votes (YES/NO/ABSTAIN using `badge-amber`/`badge-teal`/`badge-muted` respectively). Footer: APPROVE button (amber style) | DECLINE button (outline). Status badge: `IN VOTING` / `PASSED` / `OVERRIDDEN` / `DEFERRED`.

**Orchie message bubble:** Left-aligned. Background: `bg-raised`. Left border: 2px solid amber. Orchie avatar: circular logo mark, 24×24px. Text: `text-primary`, Inter Tight 13px. Timestamp: DM Mono 10px muted.

**Toast notifications:** DM Mono 12px, `radius-md`, bottom-right stack. Variants match status colours (ok/error/info/warning). Slide-up animation 0.2s.

---

## Part 3 — Prompt Templates by Asset Type

Use these prompts directly in Claude Design. Copy the prompt, fill in any bracketed placeholders, and run. All prompts are written to match the actual design system defined in Part 2.

### 3.1 Logo Generation

```
[DESIGN] "Design the Sustena logo following this system:
- Concept: a network node — a central dot with three branching arcs, suggesting a mycelium spore and a DAO governance node simultaneously
- Palette: background #0f0f0f (near-black), primary accent amber #E8A020, secondary accent teal #2ab8a0, text off-white #e8e4dc
- Typography: wordmark uses DM Mono weight 500, letter-spacing 0.08em, uppercase — 'S' in amber #E8A020, rest in #e8e4dc
- Variants to deliver: (1) full wordmark — icon left, text right, on #0f0f0f background; (2) icon only; (3) monochrome (#e8e4dc on dark, for stamps/embossing)
- The icon should be balanced and geometric — precision instrument, not organic
- Aesthetic reference: control room HUD + instrument panel + The Expanse data interface. Industrial-utilitarian."
```

### 3.2 Pitch Deck Slide Background

```
[DESIGN] "Create a slide background for a Sustena investor presentation.
- Dimensions: 1920×1080px (16:9)
- Background: near-black #0f0f0f
- Subtle texture: faint network node pattern (thin lines, circular nodes, ~6% opacity) in #e8e4dc
- Bottom accent: 2px amber line (#E8A020) at the very bottom edge
- No text, no logo — background template only
- Deliver two variants: (1) cover slide — slight amber radial glow from centre-left, more dramatic; (2) content slide — minimal texture, clean field for text overlay"
```

### 3.3 Social Media Banner (LinkedIn)

```
[DESIGN] "Create a LinkedIn company banner for Sustena Ltd.
- Dimensions: 1584×396px
- Background: #0f0f0f
- Left side: Sustena full wordmark (DM Mono weight 500, 'S' in amber #E8A020), vertically centred
- Below logo: 'Intelligence infrastructure for complex Kenyan life' — Inter Tight 18px, #8a8680
- Right side: Sustena network node icon, large, very low opacity (~15%), amber #E8A020
- Thin amber horizontal accent line below the tagline text
- Aesthetic: control room display — clean, data-forward, no decorative clutter"
```

### 3.4 Open Graph Image (sustena.io)

```
[DESIGN] "Create an Open Graph image for sustena.io.
- Dimensions: 1200×630px
- Background: #0f0f0f
- Centre: Sustena full wordmark, large — DM Mono weight 500, 'S' amber #E8A020, rest #e8e4dc
- Below wordmark: 'Intelligence infrastructure for complex Kenyan life' — Inter Tight 22px, #8a8680
- Very faint background texture: network node pattern (~5% opacity)
- Amber 2px horizontal rule between wordmark and tagline
- Must be immediately legible as a brand image when shared on social media"
```

### 3.5 UI Mockup — Budget Ring (Orchie)

```
[DESIGN] "Design the Sustena budget ring UI widget following this spec exactly:
- Background card: #181818 (--bg-surface), border: 1px solid #2e2e2e, border-radius: 6px (--radius-md), amber top-border accent: 2px solid #E8A020
- Card header: DM Mono 11px uppercase, letter-spacing 0.08em, color #8a8680 — text: 'BALANCE OVERVIEW'
- Centre of ring: liquid balance in KES — DM Mono 22px weight 500, color #e8e4dc
- Ring: donut chart, 6 pocket segments in distinct muted colours (derive from palette — all low-saturation)
- Legend: 2-column grid — pocket name (Inter Tight 12px, #565250) + allocated/spent (DM Mono 11px, #e8e4dc; amber #E8A020 for over-budget pockets)
- Mock data: KES 45,000 liquid. Pockets: Food KES 8,000/10,000, Transport KES 3,200/5,000, Rent KES 20,000/20,000, Health KES 1,500/3,000, Savings KES 10,000/10,000, Emergency KES 2,300/5,000
- Deliver: mobile card (375px) and web card (480px)"
```

### 3.6 UI Mockup — Orchie Chat Interface

```
[DESIGN] "Design a mockup of the Orchie chat interface. Follow the Sustena design system precisely:
- Overall: #0f0f0f background, mobile screen 375×812px
- Header: Orchie avatar (Sustena icon mark, 40×40px circular, amber on dark) + 'Orchie' in Inter Tight 14px weight 500 #e8e4dc + 'Your financial delegate' in DM Mono 11px #565250
- Chat history:
  * Orchie messages: left-aligned, background #181818 (bg-surface), left border 2px solid #E8A020, Inter Tight 13px #e8e4dc text, DM Mono 10px #565250 timestamp
  * User messages: right-aligned, background #212121 (bg-raised), same text style
  * Embedded widget: show one budget ring card inside an Orchie message
  * Embedded proposal: show one council proposal card inside an Orchie message
- Input area: text field (bg-surface, border 1px border, radius-md) + quick action buttons (APPROVE amber style, VOTE teal style, DETAILS muted style) — all DM Mono 11px uppercase
- Tab bar: Home | Orchie (active — amber) | Council (badge-amber '2') | Arena"
```

### 3.7 Favicon

```
[DESIGN] "Create a favicon for sustena.io using only the Sustena icon mark (network node: central dot with three branching arcs).
- Deliver: ICO at 16×16, 32×32, 48×48, and 180×180 Apple Touch Icon
- Colours: amber #E8A020 icon on #0f0f0f background
- At 16×16: simplify to central dot and three arcs only — geometric, no fine detail
- The icon must read as a distinct mark at small sizes, not a blob"
```

---

## Part 4 — Integration with Claude Code/Dispatch

### 4.1 How to Reference Assets in Code Sessions

When a Code task requires a visual asset, reference it by its path in the folder structure above. Example:

> **Correct:** "Use the logo at `C:\Users\DELL\OneDrive\Documents\Projects\Sustena XII\Sustena XII\brand\logos\sustena\sustena-icon-only.svg` as the app icon."

> **Incorrect:** "Design a new logo for the app." (This generates a new asset that isn't in the brand system.)

### 4.2 Referencing the Design System in Code Prompts

Whenever a Code prompt involves UI or styling, include this reference block. This must match the actual CSS at `ui/static/css/sustena.css` — do not modify the values here without updating both.

```
[Design system reference — Sustena]
Source CSS: ui/static/css/sustena.css (worktree hungry-ellis-c89ace)
Aesthetic: Industrial-utilitarian. Control room instrumentation, not a consumer app.
Surfaces: bg-base #0f0f0f · bg-surface #181818 · bg-raised #212121 · bg-overlay #2a2a2a
Borders: #2e2e2e · #3a3a3a · #4a4a4a
Text: primary #e8e4dc · secondary #8a8680 · muted #565250
Accent amber: #E8A020 (glow: rgba(232,160,32,0.12) / border: rgba(232,160,32,0.3))
Accent teal: #2ab8a0 (glow: rgba(42,184,160,0.1) / border: rgba(42,184,160,0.3))
Status: ok #4caf80 · warn #e8a020 · danger #e05050 · info #5090e0
Fonts: DM Mono (all data/mono values) · Inter Tight (UI chrome/labels)
Radius: sm 4px · md 6px · lg 10px
Transitions: fast 120ms ease · mid 220ms ease (state changes only, no decorative animation)
Component standards: see Sustena_Brand_Design_Assets.md §2.4
All brand assets: C:\Users\DELL\OneDrive\Documents\Projects\Sustena XII\Sustena XII\brand\
```

### 4.3 Design Asset Generation Workflow

1. Determine what asset you need (logo, UI component, social graphic, etc.)
2. Check the folder structure in Part 1 — does it already exist?
3. If not: copy the relevant prompt from Part 3; fill in any placeholders; run in Claude Design
4. Save the output to the correct subfolder (as defined in Part 1)
5. Update the folder structure in this document if you add a new asset type
6. If the asset will be used in code: add the path to the relevant Code prompt

### 4.4 When to Regenerate vs. Reuse

**Reuse** existing assets for: implementation work (putting a logo into a UI), marketing content using standard brand assets, documentation.

**Regenerate** (with the prompt from Part 3) for: a new asset type not in the folder structure, a significant brand refresh (check with Brian first), a new sustain type that needs its own icon variant.

**Never generate** a one-off visual asset for a specific post, slide, or document without saving it to the brand folder. One-off assets create brand inconsistency and design debt.

---

## Part 5 — Patent and IP Notes

*(See Sustena_Document_Analysis.md §2.4 for the Mycelium IP boundary discussion)*

**Brand IP:** The Sustena logo, wordmark, and design system are IP of Sustena Ltd. Register trademark applications for:
- "Sustena" (word mark) — Kenya Intellectual Property Institute (KIPI)
- "Orchie" (word mark) — KIPI
- "Mycelium" (for software platform context) — evaluate whether this is distinctive enough or too generic

**Platform IP:** The Sustena primitive layer, ConstraintEngine, and DSL Dot Protocol represent novel architectural work. Evaluate with a patent attorney whether the ConstraintEngine's recursive descent parser for financial system specification is patentable under Kenyan or international IP law. The DSL itself (a formal language for describing systems) may qualify for software patent protection in jurisdictions that allow it.

**Note:** Most VC-backed startups do not patent their core architecture (patents are expensive, slow, and hard to enforce). The more practical IP protection is: (a) the MIT licence on the runtime (controls how the open-source code can be used), (b) trademark registration for brand names, (c) trade secret protection for the proprietary components (Vyyb's recipe library, the Council's Nash bargaining implementation details).

---

*End of Sustena Brand Design Assets v1.1*
*Update this document whenever a new asset is generated or the design system is revised. This is a living document — its accuracy is the foundation of brand consistency.*
