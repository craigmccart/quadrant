# Quadrant Brand Identity

Status: confirmed by Craig on 2026-11-10 (dark and light). Every value is a token in `ui/css/tokens.css`.

Direction: brass instrument. A quadrant is an old navigator's instrument, so: warm near-black, bone text, one brass accent, serif numerals like engraving on a dial. Calm, no scare tactics.

Fonts (all SIL OFL 1.1, bundled in `ui/fonts`, no network needed):
- Display: Fraunces (optical size axis), weight 540. Headings, the wordmark and big numbers.
- Body: Schibsted Grotesk.
- Mono: IBM Plex Mono. Paths, sizes, registry keys.

Palette (dark theme is the default):
- Background: #131210 (sidebar #0d0c0b)
- Surface: #1a1917 / raised #22201d / hover #2c2925
- Foreground: #ece7de
- Muted: #b5ad9f
- Border: #302d28
- Accent: #d2a85c (brass). Actions, focus ring, active nav. Nothing else.
- Status only, never decoration: good #8ccb97, attention #f2934a (orange, so it never reads as the brass), problem #f2647a, info #8fb6e3

Light theme: paper #f6f3ee, surface #fffdf9, ink #1c1915, border #d6cfc3, accent #86600f (dark brass, passes contrast on white).

Radius scale: 6px controls, 10px cards, 16px hero panel.

Signature detail: the four-segment health dial. One ring, four arcs (Space, Privacy, Speed, Security), each filled from its own score, with the overall score in the middle.

Voice: plain UK English. Say what will be removed and why it is safe. No fake urgency, no exclamation marks, no red unless something is actually wrong.

Reserved for this product. Not sonofg0tham. Not any other product Craig has built.

## Design notes taken from the big apps

- CCleaner: Health Check as the front door, results grouped into a few plain categories, one big button. Version 7 added light, dark and auto themes and larger landing-page text. Users complained when it removed the file-type view from Disk Analyzer, so power tools stay.
- System Mechanic: the dashboard is the hub, with a status gauge and a history tab. Reviewers mark it down for hiding tools in menus and for locking the app during a scan.
- Fortect, Advanced SystemCare, Wise Care 365: a central ring or score, dark navy, one green or blue "go" button.
- Microsoft PC Manager: compact, calm, no upsell.
- What we copy: dashboard hub, one primary action, results sorted biggest first, every tool visible in the sidebar, the app stays usable while scanning, a history of past cleans, duplicates that keep one copy and send the rest to the Recycle Bin.
- What we refuse: scare numbers, nag screens, upsells, deleting things without showing them first.
