# Brian — Business & Operations Master Roadmap
### Sustena XII Platform Build — Phase 0 through Phase 3
**Personal document — Brian Gitonga, Co-Founder**
**Version 1.1 — May 2026**
**Cross-reference:** Sustena_XII_Master_Strategy.md §§3.2, 9.5, 11.2, 11.4 | Bonnie_Master_Roadmap.md | Colosso_Finance_Master_Document_v3.pdf (superseded by Master Strategy) | Sustena_Document_Analysis.md

---

## How to Use This Roadmap

This is your personal task sequence as finance, business operations, and marketing lead. Each task has:
- A checkbox `[ ]` (mark `[x]` when done)
- A **priority** (🔴 critical path | 🟡 important | 🟢 nice-to-have)
- A **Claude prompt** where useful: `[DISPATCH]` for research and drafting
- Subtasks with clear owners and dependencies noted

**Working principle:** You do not write code. Your outputs are: legal documents, financial models, pitch decks, social content, partner conversations, and the operational scaffolding that lets Bonnie build without distraction. When a task requires a document, use the `[DISPATCH]` prompt to draft it, then refine it yourself.

**Tools:** Use **Google Keep** for quick notes, checklists, and on-the-go captures. Google Keep syncs across devices — use it for: daily to-do lists, meeting notes, partner conversation logs, and idea capture. For structured documents and spreadsheets, use Google Docs/Sheets.

**On prompts:** Every `[DISPATCH]` prompt in this roadmap references the Sustena XII Master Strategy, the Document Analysis, and the DSL Dot Protocol. Before running a prompt, note which section of those documents is most relevant and mention it explicitly — Claude will produce better output when grounded in the actual strategy.

---

## Phase 0 — Foundation (Days 1–14)

### Epic 0.1 — Legal Entity Creation

- [ ] 🔴 **0.1.1** Identify and engage a Kenyan fintech lawyer
  - Recommended: Bowmans Kenya, Coulson Harney, or a boutique fintech practice
  - Budget: KES 50,000–80,000 for initial company formation + privacy policy
  - Timeline: initiate Week 1; expect 3–4 weeks to completion

  > **[DISPATCH]** "Using the Sustena XII Master Strategy (§3.2 entity structure and §11.1 regulatory roadmap) as context: draft a brief for a Kenyan fintech lawyer covering the incorporation of three entities: (1) Sustena Ltd — technology platform company, (2) Colosso Finance Ltd — financial services holding subsidiary, (3) 365+ Ventures Ltd — holding company. Key issues: 365+ Ventures holds 49% in Sustena Ltd and Colosso Finance Ltd; hero founders hold 51% each. Sustena Ltd will eventually operate a token-based loyalty system (pawa tokens — not yet a securities offering). Colosso Finance Ltd will in Phase 3 require CBK Digital Credit Provider licence. We need a founder agreement documenting IP assignment, equity structure, and the role of Kang'iri as strategic equity partner. List the specific documents needed and in what order, with estimated costs per document."

- [ ] 🔴 **0.1.2** eCitizen company registrations
  - Register: 365+ Ventures Ltd, Sustena Ltd, Colosso Finance Ltd
  - Cost: KES 11,750 per entity × 3 = KES 35,250
  - Requires: National ID / Passport, physical address (can use lawyer's address initially), company objectives statement

  > **[DISPATCH]** "Draft the company objectives statements for three Kenyan eCitizen BRS registrations referencing the entity structure in the Sustena XII Master Strategy §3.2: (1) Sustena Ltd — AI and software platform company (cover: development and licensing of AI software, digital platform services, decentralised network of software operators and automations). (2) Colosso Finance Ltd — a financial technology and advisory company (cover: financial intelligence software, financial data analysis, future digital credit services). (3) 365+ Ventures Ltd — a holding company (cover: holding shares in subsidiaries, strategic investment, group management services). Each statement should be 3–5 objectives in plain English suitable for the Kenya BRS form."

- [ ] 🔴 **0.1.3** NCBA Business Account opening
  - Required: Certificate of Incorporation, CR12, company resolution, director IDs
  - Do this immediately after eCitizen approval — target Week 3
  - Minimum balance: KES 10,000; maintenance: KES 500–1,000/month

- [ ] 🔴 **0.1.4** ODPC registration
  - Cost: KES 5,000/year
  - Required: Privacy Policy (drafted by lawyer), Data Protection Officer designation (Brian can be DPO)
  - Required before any user data is collected — target completion by Day 30

  > **[DISPATCH]** "Draft a Privacy Policy for Sustena Ltd referencing the data architecture in the Sustena XII Master Strategy §10.1 and the Sustena Document Analysis §4.2 (WhatsApp data handling clarification). The platform: (1) collects users' phone numbers and financial data via WhatsApp, (2) processes M-Pesa transaction histories for budgeting analysis, (3) stores user-generated financial data (budgets, spending records, chama contributions) in a cloud database, (4) offers users the option to anonymise and license their data to research institutions in exchange for pawa token rewards. Must comply with Kenya's Data Protection Act 2019. Include: data collected, purpose of collection, legal basis, data sharing, user rights (access, correction, deletion, withdrawal of consent), data retention, DPO contact details, data portability (per Document Analysis §4.3 — users can export their sustain state at any time). Also include a plain-English 'Trust and Safety' section (per Document Analysis §4.1 framing: 'Orchie is like a brilliant accountant who can see your books but cannot sign the cheques')."

- [ ] 🔴 **0.1.5** Founder agreement
  - Parties: Bonventure Gachiengu, Brian Gitonga, Kang'iri
  - Key terms: IP assignment, equity split, vesting (4-year cliff-1 for both founders), Kang'iri's strategic equity role at 365+ level

  > **[DISPATCH]** "Draft a Founder Agreement term sheet for three co-founders of a Kenyan tech startup per the structure in Sustena XII Master Strategy §3.2: (1) Bonventure Gachiengu — technical co-founder, systems architect, 51% of Sustena Ltd and Colosso Finance Ltd. (2) Brian Gitonga — business co-founder, finance and operations lead, co-owner of Sustena Ltd and Colosso Finance Ltd. (3) Kang'iri — strategic equity partner at 365+ Ventures Ltd level only. 365+ Ventures Ltd holds 49% of each subsidiary. Key terms: IP assignment (all existing and future platform IP owned by respective company), vesting (4-year vest, 1-year cliff, monthly thereafter — for Bonventure and Brian), drag-along rights, tag-along rights, decision-making (ordinary: majority vote; extraordinary: unanimous consent including acquisition offers above $1M). Good-leaver / bad-leaver definitions. Format as a term sheet — lawyer will draft the final."

---

### Epic 0.2 — Financial Model and Budget Management

- [ ] 🔴 **0.2.1** Phase 0 financial model

  > **[DISPATCH]** "Build a 12-month financial projection for Sustena Ltd referencing the revenue model in Sustena XII Master Strategy §9.1 and the resource assessment in the Document Analysis §6.3. Inputs: starting cash KES 0 (bootstrapped), Bonnie's deferred salary KES 200,000/month (accrued, not paid), Brian's deferred salary KES 180,000/month (accrued, not paid). Monthly costs: Claude API $10–50, Google Cloud Run $5–15, NCBA account KES 750/month. Revenue projections: Phase 0 (Days 1–30) KES 0. Phase 1 (Month 1–2): 50 subscribers × KES 200/month. Month 3: 200 × KES 200. Month 6: 500 × KES 250 + 20 chamas × KES 2,000 + 30 businesses × KES 1,000. Show: monthly P&L, cumulative cash position, cumulative accrued liabilities, break-even month. The 60-day window to first KES 50,000 MRR is the critical constraint — model it explicitly."

- [ ] 🔴 **0.2.2** Expense tracking — Google Keep + Google Sheets
  - Use Google Keep for daily expense capture (photo receipts, voice notes, quick amounts)
  - Weekly: transfer Keep notes into a Google Sheet — categories: Legal fees, eCitizen, Domain registration, Software subscriptions, Travel/meetings
  - All expenses shared between founders via WhatsApp receipt photos → weekly reconciliation

- [ ] 🟡 **0.2.3** Fundraising target tracking — Google Sheets
  - Maintain a live Fundraising Tracker: investor name, conversation stage, ask amount, expected close date
  - Update weekly; share with Bonnie weekly
  - Use Google Keep for quick notes from investor conversations; transfer to the tracker same day

---

### Epic 0.3 — Domain Registration

- [ ] 🔴 **0.3.1** Register sustena.io (or sustena.ai)
  - Primary preference: `sustena.ai` ($50–90/year via Namecheap)
  - Backup: `sustena.io` ($35–45/year)
  - Register on Day 1 — before any public announcement
  - Also register: `vyyb.co.ke`, `colosso.finance`, `colosso.ai`, `365plus.io`
  - After registering, hand DNS credentials to Bonnie for Cloudflare setup

  > **[DISPATCH]** "Research the current availability and pricing of the following domains: sustena.ai, sustena.io, vyyb.co.ke, colosso.finance, colosso.ai, 365plus.io. For each: is it available? Registration price (first year + renewal)? Which registrar is recommended? What DNS setup is needed if we use Cloudflare for CDN? Provide a ranked recommendation: which to register immediately (Day 1), which to register in Month 1, which are optional. Reference the brand identity system in the Sustena XII Master Strategy §12 for context on brand priority."

---

## Phase 1 — Foundation Build (Weeks 1–10)

### Epic 1.1 — User Acquisition (First 500 Users)

- [ ] 🔴 **1.1.1** WhatsApp pilot — 30 beta users
  - Target: personal network first (friends, family, chama members, business contacts)
  - Recruitment message: personal WhatsApp (not broadcast); explain it's a beta
  - Goal: 30 active users in Week 1–2; collect feedback daily via Google Keep

  > **[DISPATCH]** "Write a WhatsApp message to recruit beta testers for Sustena using the brand voice in the Sustena XII Master Strategy §12.1 (honest, confident, community-first). The message should: (1) be informal, personal, and Kenyan in tone (Swahili/English mix acceptable), (2) explain the value: Orchie helps manage budget, track spending, and plan money — all through WhatsApp, (3) be honest that it's a beta and we want feedback, (4) clear CTA: 'Reply YES and I'll add you.' Under 150 words. Do not oversell. Do not promise features not yet built."

- [ ] 🔴 **1.1.2** Chama acquisition strategy — 20 chamas by Month 3
  - Target: chamas in personal network and church/community groups
  - Pitch: Orchie as the chama secretary — automated contribution tracking, loan records, meeting scheduling
  - Offer Month 1–2 free for early chamas in exchange for honest feedback
  - Log all chama contacts in Google Keep; transfer to Google Sheets tracker weekly

  > **[DISPATCH]** "Write a one-page pitch for Sustena's Chama Secretary service referencing the chama operative in Sustena XII Master Strategy §8.5 and the chama-as-acquisition-channel framing in §11.4 (chama multiplier: 20 chamas = 200–400 warm users). The pitch should: (1) open with the problem (manual records, disputes, misappropriation), (2) present Orchie as the solution, (3) include concrete examples of what Orchie does, (4) pricing: KES 1,500/month for the first 6 months, (5) CTA: 'Try free for 30 days.' Professional but warm Kenyan English. One page maximum."

- [ ] 🔴 **1.1.3** Biashara SaaS — 30 small businesses by Month 3
  - Target: existing Vyyb food business contacts, MSME networks, market traders
  - Focus on businesses with a WhatsApp presence

  > **[DISPATCH]** "Write a 60-second WhatsApp voice note script pitching Sustena Biashara to a small business owner referencing the Biashara sustain in Sustena XII Master Strategy §8.6/8.8. The script: (1) pain point: juggling inventory, daily revenue, KRA, all in your head, (2) the solution: Orchie sends a daily summary every evening via WhatsApp — what sold, margins, what to reorder, upcoming tax dates, (3) setup: 'Takes 10 minutes to set up. We do it with you.', (4) cost: KES 500/month, (5) Try free: first 30 days free. Natural spoken pitch, include pauses marked with [pause]."

---

### Epic 1.2 — Partnership Development

- [ ] 🟡 **1.2.1** NCBA partnership — SME and chama banking

  > **[DISPATCH]** "Draft a partnership proposal email to NCBA Bank Kenya's Head of SME Banking grounding the pitch in Sustena's ODPC compliance and WhatsApp-first strategy (Sustena XII Master Strategy §10.1 and §11.3). Sustena's Biashara Sustain manages the business intelligence layer for MSMEs; NCBA provides banking infrastructure. Referral flow: Sustena users needing a business account are referred to NCBA (Sustena earns referral fee); NCBA SME clients needing business intelligence are referred to Sustena. Key points: (1) ODPC compliance, (2) WhatsApp-first matches NCBA's digital-first strategy, (3) targeting 500 MSMEs in Phase 1. Under 300 words. Ask for a 30-minute meeting."

- [ ] 🟡 **1.2.2** Strathmore University — research data partnership

  > **[DISPATCH]** "Draft a partnership proposal to Strathmore University's Institute of Mathematical Sciences referencing the data licensing model in Sustena XII Master Strategy §9.3. Sustena will accumulate (with explicit user consent) anonymised data on: household spending patterns, chama contribution and loan repayment behaviour, small business cash flow, agricultural supply chain pricing (Mkulima sustain). Propose a data partnership: Strathmore researchers submit queries via licensed API; Sustena provides anonymised, aggregated results; Strathmore provides academic validation and co-authorship. No user-identifiable data ever shared. Propose a 12-month pilot. Ask for a meeting with the research director."

- [ ] 🟢 **1.2.3** IFC / AGRA / Mastercard Foundation outreach (Mkulima angle)
  - These are 12–18 month conversations — start early; relationship-building first, not funding

  > **[DISPATCH]** "Draft a one-page executive summary of the Mkulima Sustain referencing Sustena XII Master Strategy §8.7. Cover: (1) problem: smallholder farmers lack real-time market price intelligence, (2) solution: Mkulima Sustain — AI farm manager tracking crop cycles, receiving buyer POs from Vyyb and other Biashara sustains, broadcasting supply signals, managing farm finances via M-Pesa, (3) data advantage: as Mkulima sustains scale, Sustena builds the most detailed real-time agricultural supply chain dataset in East Africa, (4) ask: discussing a potential pilot programme with 50–100 smallholder farmers. One page, investor-quality English."

---

### Epic 1.3 — Operations and Metrics

- [ ] 🔴 **1.3.1** Weekly metrics dashboard (Google Sheets)
  - Metrics: MAU, new signups/week, 30-day retention rate, MRR, churn rate, NPS (monthly survey)
  - Use Google Keep to log observations during the week; compile into Sheets every Monday
  - Update every Monday; share with Bonnie; review together Friday

- [ ] 🔴 **1.3.2** User feedback logging (no customer support)
  - **Note:** Brian does not manage customer support (that is Bonnie's domain — she is closer to the product). Brian's role: log feedback signals from his business conversations and pass them to Bonnie weekly.
  - WhatsApp group for beta users: "Sustena Beta Feedback" (Bonnie moderates technical issues; Brian monitors for business/UX patterns)
  - Brian's weekly action: note recurring themes, testimonials, and business-need signals in Google Keep → send to Bonnie every Friday

- [ ] 🟡 **1.3.3** User onboarding outreach (Brian's personal touch)
  - Every new user who signs up via Brian's referrals gets a personal WhatsApp from Brian within 2 hours
  - Message: "Karibu Sustena! Mimi ni Brian, moja wa waanzilishi. Nataka kukusaidia uanze vizuri. Je, unaanza kwa Homestead (fedha za nyumbani) au Biashara?"
  - Technical onboarding is handled by Orchie; Brian's touch is relationship-building only

---

### Epic 1.4 — Investor Readiness

- [ ] 🟡 **1.4.1** Investor deck (first version)

  > **[DISPATCH]** "Write the narrative content for a 12-slide seed investor pitch deck for Sustena Ltd. Ground every slide in the Sustena XII Master Strategy. Use the investor challenge responses from the Investor Challenges document (separate file) to pre-empt objections. Slide titles and key points: (1) Cover: 'Sustena — Intelligence infrastructure for complex African life'. (2) Problem: The cost of complexity for Kenyan households and businesses. (3) Solution: Orchie — the AI delegate that manages your sustain (plain language, no jargon). (4) Product demo: 3 use cases — Homestead budget ring, Chama contribution tracking, Biashara daily revenue summary. (5) Market: Kenya's 50M population, 3.5M MSMEs, 300,000+ chamas, East Africa TAM. (6) Traction: [leave blank — insert real numbers]. (7) Business model: subscription tiers + chama rates + Biashara SaaS + data licensing (Phase 2) + pawa token network (Phase 3). (8) Technology: 3 bullets max on Sustena primitives, Mycelium, web3 roadmap. (9) Team: Bonventure (systems architect) + Brian (finance lead) + Kang'iri (strategic equity). (10) Financials: 12-month projection + funding ask ($350,000) + use of funds. (11) Roadmap: Phase 0 → Phase 1 → Phase 2 → Phase 3. (12) Ask: $350,000 seed, 15–20% dilution. Write speaker notes + key point per slide, not full slide text. Include the 'Why Not Incumbents' answer from Document Analysis §3.1 in slide 3 or 8."

- [ ] 🟡 **1.4.2** Investor pipeline
  - Tier 1 (East Africa tech VCs): DOB Equity, Novastar, Seedstars Africa, AfricArena
  - Tier 2 (impact investors): Acumen Fund, Omidyar Network, Mastercard Foundation Ventures
  - Tier 3 (angels): Kenyan tech founders, diaspora investors
  - Action: send cold email + deck to 5 Tier 1 investors in Month 2

  > **[DISPATCH]** "Write a cold outreach email to Novastar Ventures referencing Sustena XII Master Strategy §11.4 (investor targeting rationale) and the moat analysis in the Investor Challenges document (moat: personalised state data + operative ecosystem + cross-sustain network effects). Open with traction (insert real numbers — write placeholder), explain Sustena in one sentence, briefly note the market, explain why Novastar specifically. Ask for a 20-minute call. Under 250 words. No attachments on first email."

---

### Epic 1.5 — Content and Brand Stories

Brian's content lane is **business storytelling** — not technical writing, not how-to guides, not educational content (that is Bonnie's lane). Brian's posts tell the story of Sustena's momentum: what happened, who believed in it, what it means.

- [ ] 🟡 **1.5.1** LinkedIn post series — Sustena business milestones
  - Post types: closed a new partnership, passed a user milestone, a client story, an on-brand reflection
  - Cadence: 1–2 posts per week on LinkedIn (Brian's personal account + Sustena company page when live)
  - Tone: honest, confident, proud but not boastful; Kenyan entrepreneur voice

  > **[DISPATCH]** "Write 4 LinkedIn post templates for Brian Gitonga, co-founder of Sustena, grounded in the brand voice in Sustena XII Master Strategy §12.1. One template for each: (1) New partnership announcement — structure: what we announced, what it means for our users, what it means for our vision. Tone: excited but grounded. (2) User/subscriber milestone — structure: the number, one story from a real user (anonymised), what we're building toward. Tone: grateful, momentum-building. (3) Client success story — structure: the problem the client had, how Orchie helped, the outcome in concrete terms (KES saved, hours reclaimed, decisions made). Anonymised. (4) On-brand reflection — structure: something Brian noticed about the Kenyan economy/small business ecosystem, why it connects to Sustena's thesis, a call to follow or try the product. Tone: thoughtful, not preachy. Each template should be 150–250 words, optimised for LinkedIn engagement (no bullet walls, strong opening line, clear close)."

  > **[DISPATCH]** "Write a LinkedIn post for Brian Gitonga announcing Sustena's first closed partnership referencing the NCBA partnership rationale in Sustena XII Master Strategy §11.3. Structure: one-sentence hook about what just happened, two sentences on what this means for Kenyan MSMEs, one sentence on what's next. Tone: excited but grounded, Kenyan entrepreneur voice. Under 200 words. Do not name the partner if the partnership is not yet publicly confirmed — use 'a leading Kenyan bank.'"

- [ ] 🟡 **1.5.2** WhatsApp Status updates — Sustena community building
  - Weekly WhatsApp Status post from Brian: a milestone, a user quote, a "building in public" moment
  - Audience: Brian's personal network = the first 300 potential users
  - Keep in Google Keep as a running list; post every Monday morning

---

## Phase 2 — Platformisation (Months 3–6)

### Epic 2.1 — Sales and Revenue Scale

- [ ] 🟡 **2.1.1** Formalise pricing and payment infrastructure
  - Specify the payment flow; Bonnie implements M-Pesa Daraja integration
  - Pricing page on sustena.io: Personal (KES 150/month), Chama (KES 2,000/month), Biashara (KES 1,000/month)
  - Define failed payment → 3-day grace → account suspended flow

- [ ] 🟡 **2.1.2** Mkulima Premium launch — 500 farmers by Month 6
  - Partnership with agricultural extension officers
  - Pilot: 50 farmers in one value chain (tomatoes: Nairobi supply catchment)
  - Brian visits the area, does in-person WhatsApp setup with 5 farmers; they train the rest

- [ ] 🟡 **2.1.3** B2B pipeline — food buyer network
  - Target: restaurant chains, supermarkets (Naivas, Quickmart), hotel procurement teams
  - Pitch: Vyyb's Mkulima integration gives them pre-harvest procurement at below-market prices
  - Business model: 2% commission on each PO facilitated through the platform

- [ ] 🔴 **2.1.4** Hiring — first non-founder team members
  - Target Month 4 (post-seed): Customer Success Manager (KES 80,000–120,000/month)
  - Target Month 6: Marketing Associate (KES 60,000–80,000/month)

  > **[DISPATCH]** "Write a job description for a Customer Success Manager at Sustena Ltd referencing the platform's user base (Homestead, Chama, Biashara sustains) from Sustena XII Master Strategy §8. Responsibilities: (1) manage WhatsApp-based customer support for 1,000+ users, (2) conduct onboarding calls for chama and Biashara tier clients, (3) collect and synthesise product feedback into a weekly brief for the founders (primarily to Bonnie for technical triage), (4) own retention metrics: 30-day retention, NPS, churn. Requirements: 2+ years customer success or support experience, excellent written and verbal Swahili + English, familiarity with M-Pesa and basic financial concepts. Salary: KES 80,000–120,000/month + equity (0.25–0.5% vesting 4 years). Location: Nairobi (hybrid)."

---

### Epic 2.2 — Brand and Business Reporting

- [ ] 🟡 **2.2.1** Monthly milestone brief — business narrative
  - Brian's role: gather the month's business story (partnerships closed, client wins, business metrics)
  - Hand the brief to Bonnie, who publishes the full blog post and developer update
  - Keep it in Google Keep throughout the month; compile on the last Friday

- [ ] 🟡 **2.2.2** Community events — quarterly Sustena meetups
  - Nairobi-based: invite beta users, chamas, Biashara clients, developers
  - Format: 2-hour event — product demo (30min), panel Q&A with users (30min), networking (60min)
  - Brian runs logistics and community outreach; Bonnie runs the product demo

---

### Epic 2.3 — Regulatory and Compliance Milestones

- [ ] 🟡 **2.3.1** CBK Digital Credit Provider licence preparation
  - Start in Month 4 (Phase 2) — takes 12–18 months
  - Requirements: KES 20M+ capital in reserve, robust KYC/AML system, audited financials

  > **[DISPATCH]** "Create a CBK Digital Credit Provider (DCP) licence preparation checklist for Colosso Finance Ltd referencing the regulatory roadmap in Sustena XII Master Strategy §11.1. Cover requirements under the Central Bank of Kenya (Amendment) Act 2021: (1) capital requirements (KES 20M minimum), (2) fit and proper test for directors, (3) data governance requirements, (4) interest rate cap compliance, (5) KYC/AML requirements, (6) application documents required. For each: what it is, what we need to prepare, estimated timeline. Note which requirements Sustena's ODPC registration already satisfies."

- [ ] 🟡 **2.3.2** CMA Sandbox application (for pawa/STC token framework)
  - File in Month 6–9 — after Phase 1 live data demonstrates utility
  - Frame pawa tokens as loyalty points (not securities) in Phase 1

---

## Phase 3 — Decentralisation (Month 12+)

### Epic 3.1 — STC Launch and Token Economy

- [ ] 🟢 **3.1.1** STC tokenomics document (co-authored with Bonnie)
  - Brian owns the economic model; Bonnie owns the technical implementation
  - Sections: token allocation, vesting schedule, conversion rate methodology, governance rights, use of treasury

  > **[DISPATCH]** "Draft a Sustena Token (STC) tokenomics document for internal review referencing the pawa system in Sustena XII Master Strategy §9.5 and the CMA Sandbox strategy in §11.1. STC is a governance token (not a security — explicitly disclaim). Sections: (1) Purpose: STC gives holders voting rights on Mycelium governance parameters. (2) Supply: fixed cap of 100,000,000 STC. (3) Allocation: suggest allocations for team/founders, community contributors, ecosystem fund, treasury, public sale. (4) Vesting: suggest appropriate vesting per category. (5) Pawa-to-STC conversion: describe the conversion mechanism (DAO-governed rate, conversion windows). (6) Governance rights: what STC holders can vote on, quorum, timelock. (7) What STC is NOT. Note: subject to legal review for Kenya CMA Sandbox compliance."

- [ ] 🟢 **3.1.2** VASP partnership for KES off-ramp
  - Research and shortlist Kenyan VASPs with CBK recognition
  - Brian leads commercial negotiation; Bonnie handles technical API integration

---

## Quick Reference — Brian's Weekly Routine

**Monday:**
- Update metrics dashboard (Google Sheets) with MAU, MRR, churn, NPS
- Log week's priorities in Google Keep
- Send weekly investor update (once fundraising starts)
- Review product feedback summary from Bonnie

**Wednesday:**
- Conduct 2–3 user calls (chama chairpersons or Biashara owners)
- Review partnership pipeline; send follow-up emails
- Capture notes in Google Keep immediately after each call

**Friday:**
- Compile week's user/business observations for Bonnie (30-minute sync)
- Draft LinkedIn post for next week
- Update fundraising tracker in Google Sheets

**Monthly:**
- Compile monthly milestone brief for Bonnie's blog post
- Financial reconciliation (NCBA statement vs. expense tracker)
- Investor update (if in fundraising mode)
- ODPC data processing register update

---

*End of Brian Master Roadmap v1.1*
*Updated: May 2026. Changes from v1.0: Removed 0.3.2 (brand brief — reassigned to Bonnie). Replaced Google Sheets with Google Keep for daily capture. Removed Substack/blog setup (Bonnie's domain). Clarified customer support is Bonnie's domain; Brian logs feedback signals only. Updated all [DISPATCH] prompts to reference source documents. Brian's content lane defined as business storytelling (partnerships, milestones, client stories).*
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 