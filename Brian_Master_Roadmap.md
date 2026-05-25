# Brian — Business & Operations Master Roadmap
### Sustena XII Platform Build — Phase 0 through Phase 3
**Personal document — Brian Gitonga, Co-Founder**
**Version 1.0 — May 2026**
**Cross-reference:** Sustena_XII_Master_Strategy.md, Sections 3.2, 9.5, 11.2, 11.4

---

## How to Use This Roadmap

This is your personal task sequence as finance, business operations, and marketing lead. Each task has:
- A checkbox `[ ]` (mark `[x]` when done)
- A **priority** (🔴 critical path | 🟡 important | 🟢 nice-to-have)
- A **Claude prompt** where useful: `[DISPATCH]` for research and drafting, `[DESIGN]` for visuals
- Subtasks with clear owners and dependencies noted

**Working principle:** You do not write code. Your outputs are: legal documents, financial models, pitch decks, social content, partner conversations, and the operational scaffolding that lets Bonnie build without distraction. When a task requires a document, use the `[DISPATCH]` prompt to draft it, then refine it yourself.

---

## Phase 0 — Foundation (Days 1–14)

### Epic 0.1 — Legal Entity Creation

- [ ] 🔴 **0.1.1** Identify and engage a Kenyan fintech lawyer
  - Recommended: Bowmans Kenya, Coulson Harney, or a boutique fintech practice
  - Budget: KES 50,000–80,000 for initial company formation + privacy policy
  - Timeline: initiate Week 1; expect 3–4 weeks to completion

  > **[DISPATCH]** "Draft a brief for a Kenyan fintech lawyer covering the incorporation of three entities: (1) Sustena Ltd — technology platform company, (2) Colosso Finance Ltd — financial services holding subsidiary, (3) 365+ Ventures Ltd — holding company. Key issues to address: (a) 365+ Ventures holds 49% in Sustena Ltd and Colosso Finance Ltd; hero founders hold 51% each. (b) Sustena Ltd will eventually operate a token-based loyalty system (pawa tokens — not yet a securities offering). (c) Colosso Finance Ltd will in Phase 3 require CBK Digital Credit Provider (DCP) licence. (d) We need a founder agreement documenting IP assignment, equity structure, and the role of Kang'iri as strategic equity partner. List the specific documents we need prepared and in what order, with estimated costs per document."

- [ ] 🔴 **0.1.2** eCitizen company registrations
  - Register: 365+ Ventures Ltd, Sustena Ltd, Colosso Finance Ltd
  - Cost: KES 11,750 per entity × 3 = KES 35,250
  - Requires: National ID / Passport, physical address (can use lawyer's address initially), company objectives statement

  > **[DISPATCH]** "Draft the company objectives statements for three Kenyan eCitizen BRS registrations: (1) Sustena Ltd — an AI and software platform company. Objectives should cover: development and licensing of artificial intelligence software, provision of digital platform services, management of a decentralised network of software operators and automations. (2) Colosso Finance Ltd — a financial technology and advisory company. Objectives: financial intelligence software, financial data analysis, (future) digital credit services. (3) 365+ Ventures Ltd — a holding company. Objectives: holding shares in subsidiaries, strategic investment, group management services. Each statement should be 3–5 objectives in plain English suitable for the Kenya BRS form."

- [ ] 🔴 **0.1.3** NCBA Business Account opening
  - Required: Certificate of Incorporation, CR12, company resolution, director IDs
  - Do this immediately after eCitizen approval — target Week 3
  - Minimum balance: KES 10,000; maintenance: KES 500–1,000/month

- [ ] 🔴 **0.1.4** ODPC (Office of the Data Protection Commissioner) registration
  - Cost: KES 5,000/year
  - Required: Privacy Policy (drafted by lawyer), Data Protection Officer designation (Brian can be DPO)
  - Required before any user data is collected — target completion by Day 30

  > **[DISPATCH]** "Draft a Privacy Policy for Sustena Ltd, a Kenyan AI platform that: (1) collects users' phone numbers and financial data via WhatsApp, (2) processes M-Pesa transaction histories for budgeting analysis, (3) stores user-generated financial data (budgets, spending records, chama contributions) in a cloud database, (4) offers users the option to anonymise and license their data to research institutions in exchange for pawa token rewards. The policy must comply with Kenya's Data Protection Act 2019. Include sections: data collected, purpose of collection, legal basis, data sharing (research institution licensing with explicit consent), user rights (access, correction, deletion, withdrawal of consent), data retention periods, DPO contact details. Use plain language — the primary audience is a Kenyan small business owner or chama member."

- [ ] 🔴 **0.1.5** Founder agreement
  - Parties: Bonventure Gachiengu, Brian Gitonga, Kang'iri
  - Key terms: IP assignment (all platform code and documents assigned to Sustena Ltd), equity split (365+: 49%, hero founder: 51% per subsidiary), vesting schedule (4-year cliff-1 for both founders), Kang'iri's role as strategic equity partner at 365+ level
  - Do NOT skip the vesting schedule — it protects both founders

  > **[DISPATCH]** "Draft a Founder Agreement term sheet for three co-founders of a Kenyan tech startup. Structure: (1) Bonventure Gachiengu — technical co-founder, systems architect, 51% of Sustena Ltd and Colosso Finance Ltd. (2) Brian Gitonga — business co-founder, finance and operations lead, co-owner of Sustena Ltd and Colosso Finance Ltd. (3) Kang'iri — strategic equity partner at 365+ Ventures Ltd level only (not an operator in the subsidiaries). 365+ Ventures Ltd holds 49% of each subsidiary. Key terms to include: IP assignment (all existing and future platform IP is owned by the respective company, not individually), vesting (4-year vest, 1-year cliff, monthly thereafter — for both Bonventure and Brian), drag-along rights, tag-along rights, decision-making (ordinary: majority vote; extraordinary: unanimous consent including acquisition offers above $1M). Good-leaver / bad-leaver definitions. Format as a term sheet, not a final agreement — the lawyer will draft the final."

---

### Epic 0.2 — Financial Model and Budget Management

- [ ] 🔴 **0.2.1** Phase 0 financial model

  > **[DISPATCH]** "Build a 12-month financial projection spreadsheet for Sustena Ltd. Inputs: starting cash KES 0 (bootstrapped), Bonnie's deferred salary KES 200,000/month (accrued, not paid), Brian's deferred salary KES 180,000/month (accrued, not paid). Monthly costs: Claude API $10–50 (use midpoint), Google Cloud Run $5–15, NCBA account KES 750/month. Revenue projections by phase: Phase 0 (Days 1–30): KES 0 revenue. Phase 1 (Month 1–2): 50 subscribers × KES 200/month = KES 10,000 MRR. Month 3: 200 × KES 200 = KES 40,000. Month 6: 500 × KES 250 + 20 chamas × KES 2,000 + 30 businesses × KES 1,000 = KES 195,000. Show: monthly P&L (revenue − cash costs), cumulative cash position, cumulative accrued liabilities (deferred salaries), break-even month. Output as a table I can put in a spreadsheet."

- [ ] 🔴 **0.2.2** Petty cash and expense tracking system
  - Set up a simple Google Sheet for Phase 0 expenses (before Colosso Finance tools are live)
  - Categories: Legal fees, eCitizen, Domain registration, Software subscriptions, Travel/meetings
  - All expenses shared between founders via WhatsApp receipt photos → weekly reconciliation

- [ ] 🟡 **0.2.3** Fundraising target tracking
  - Maintain a live Fundraising Tracker: investor name, conversation stage, ask amount, expected close date
  - Update weekly; share with Bonnie weekly

---

### Epic 0.3 — Domain Registration and Brand Setup

- [ ] 🔴 **0.3.1** Register sustena.io (or sustena.ai)
  - Primary preference: `sustena.ai` ($50–90/year via Namecheap)
  - Backup: `sustena.io` ($35–45/year)
  - Register on Day 1 — before any public announcement
  - Also register: `vyyb.co.ke`, `colosso.finance`, `colosso.ai`, `365plus.io`

  > **[DISPATCH]** "Research the current availability and pricing of the following domains: sustena.ai, sustena.io, vyyb.co.ke, colosso.finance, colosso.ai, 365plus.io. For each: is it available? What is the registration price (first year + renewal)? Which registrar is recommended? What DNS setup is needed if we use Cloudflare for CDN? Provide a ranked recommendation: which to register immediately (Day 1), which to register in Month 1, which are optional."

- [ ] 🟡 **0.3.2** Brand identity brief for Claude Design
  - Write the brief before Bonnie runs Claude Design sessions
  - Core brief elements: Sustena's positioning ("intelligence infrastructure for complex Kenyan life"), aesthetic (The Expanse-inspired, earthy mycelium palette), target user (urban Kenyan professional 25–45, chama member, small business owner), colours (dark #1a1a2e background, amber accent #f5a623, mycelium green #4a7c59)

  > **[DESIGN]** "Design the Sustena brand identity system. The brand should evoke: precision + organic growth + African rootedness + sci-fi ambition. Primary palette: deep space navy (#1a1a2e), mycelium amber (#f5a623), forest green (#4a7c59), off-white (#e8e8d5). Typography: a geometric sans-serif for headings (suggest Sora or Space Grotesk), monospace for data/code displays (JetBrains Mono). Logo concept: a stylised network node — a dot at centre with three branching arcs, suggesting both a mycelium spore and a DAO governance node. Deliver: (1) logo in 3 variants (full wordmark, icon only, monochrome), (2) colour palette with accessibility contrast ratios, (3) typography specimen, (4) sample sustain UI card using the system."

---

## Phase 1 — Foundation Build (Weeks 1–10)

### Epic 1.1 — User Acquisition (First 500 Users)

- [ ] 🔴 **1.1.1** WhatsApp pilot — 30 beta users
  - Target: your personal network first (friends, family, chama members, business contacts)
  - Recruitment message: personal WhatsApp (not broadcast); explain it's a beta
  - Goal: 30 active users in Week 1–2; collect feedback daily

  > **[DISPATCH]** "Write a WhatsApp message to recruit beta testers for Sustena — a new AI-powered financial management assistant for Kenyans, delivered via WhatsApp. The message should: (1) be informal, personal, and Kenyan in tone (mix of Swahili and English is acceptable), (2) explain the value clearly: Orchie helps you manage your budget, track spending, and plan your money — all through WhatsApp, (3) be honest that it's a beta and we want feedback, (4) have a clear CTA: 'Reply YES and I'll add you.' Keep it under 150 words. Do not oversell. Do not promise features that aren't built yet (Phase 0: only basic budget tracking via WhatsApp)."

- [ ] 🔴 **1.1.2** Chama acquisition strategy — 20 chamas by Month 3
  - Target: chamas in your personal network and church/community groups
  - Pitch: Orchie as the chama secretary — automated contribution tracking, loan records, meeting scheduling
  - Offer Month 1–2 free for early chamas in exchange for honest feedback

  > **[DISPATCH]** "Write a one-page pitch for Sustena's Chama Secretary service to present to chama chairpersons. The pitch should: (1) open with the problem (chama secretaries spend hours on manual records, disputes arise from incorrect records, money is sometimes misappropriated), (2) present Orchie as the solution (AI chama secretary — tracks contributions automatically, sends reminders, generates monthly statements, manages loan records), (3) include concrete examples of what Orchie can do: 'Remind all members 3 days before contribution date. Record Brian's KES 2,000 contribution with a receipt. Generate April statement showing who has paid, who owes, and current loan book.', (4) pricing: KES 1,500/month for the first 6 months (introductory), (5) call-to-action: 'Try free for 30 days.' Write in a professional but warm Kenyan English tone. One page maximum."

- [ ] 🔴 **1.1.3** Biashara SaaS — 30 small businesses by Month 3
  - Target: existing Vyyb food business contacts, MSME networks, market traders
  - Focus on businesses with a WhatsApp presence (which is almost all of them)
  - Pitch: Orchie as their business intelligence — inventory alerts, daily revenue summary, KRA reminders

  > **[DISPATCH]** "Write a 60-second WhatsApp voice note script (to be read naturally, not robotically) pitching Sustena Biashara to a small business owner. The business owner sells food (similar to Vyyb). The script should cover: (1) the pain point: 'You know how you're always trying to remember what sold today, what needs restocking, whether VAT is due?' (2) the solution: Orchie sends you a daily summary every evening — what sold, margins, what to reorder, upcoming tax dates. All on WhatsApp. (3) setup: 'Takes 10 minutes to set up. We do it with you.' (4) cost: KES 500/month. (5) Try free: first 30 days free. Write the script as a natural spoken pitch, not a written advert. Include natural pauses marked with [pause]."

- [ ] 🟡 **1.1.4** Content marketing — Sustena Substack + blog setup
  - Create Substack: "The Sustena Brief" — monthly long-form on AI, money, and Kenyan systems
  - Create sustena.io/blog — automated platform milestones, event calendar
  - Posting cadence: Substack monthly, blog fortnightly

  > **[DISPATCH]** "Write the first Sustena Substack article. Title: 'We're building the nervous system for Kenyan businesses — here's why.' The article should: (1) open with a vivid image of the complexity of running a small Kenyan business (juggling M-Pesa, inventory, staff, KRA, chama, rent — all in your head), (2) introduce Sustena's thesis: the problem isn't that Kenyans lack financial intelligence — it's that the tools they have don't think, (3) explain Orchie in plain terms (not technical), (4) share an honest picture of where we are (early, building in public, looking for beta testers), (5) close with a CTA to join the beta and follow the Substack. Tone: honest, confident, slightly idealistic. Length: 600–800 words. First-person (Brian's voice)."

---

### Epic 1.2 — Partnership Development

- [ ] 🟡 **1.2.1** NCBA partnership — SME and chama banking
  - NCBA already has an M-Pesa integration and SME banking product
  - Target: referral agreement — Sustena refers businesses, NCBA refers their SME clients to Sustena
  - Contact: NCBA Business Banking team (LinkedIn outreach)

  > **[DISPATCH]** "Draft a partnership proposal email to NCBA Bank Kenya's Head of SME Banking. Sustena is proposing a referral partnership: Sustena's Biashara Sustain platform manages the business intelligence layer for MSMEs; NCBA provides the banking infrastructure. Referral flow: Sustena users who need a business account are referred to NCBA (Sustena earns referral fee); NCBA SME clients who need business intelligence tools are referred to Sustena (NCBA earns referral fee or co-branded discount). Key points to include: (1) Sustena's ODPC registration (data protection compliance), (2) our WhatsApp-first approach matches NCBA's digital-first strategy, (3) we are targeting 500 MSMEs in Phase 1 — potential NCBA account openings. Keep email professional, under 300 words. Ask for a 30-minute meeting."

- [ ] 🟡 **1.2.2** Strathmore University — research data partnership
  - Strathmore's Institute of Mathematical Sciences + @iLabAfrica are potential partners
  - Value exchange: Sustena provides anonymised economic data; Strathmore provides academic validation

  > **[DISPATCH]** "Draft a partnership proposal to Strathmore University's Institute of Mathematical Sciences. Sustena is building a platform where Kenyan households and businesses manage their finances via AI. We will accumulate (with explicit user consent) anonymised data on: household spending patterns by category and income bracket in Nairobi; chama contribution and loan repayment behaviour; small business cash flow and demand patterns; agricultural supply chain pricing (Mkulima sustain). We propose a data partnership: Strathmore researchers can submit data queries through a licensed API; Sustena provides anonymised, aggregated results; Strathmore provides academic validation and co-authorship on research papers (which we use for investor credibility). No user-identifiable data is ever shared. Propose a 12-month pilot agreement. Ask for a meeting with the research director."

- [ ] 🟢 **1.2.3** IFC / AGRA / Mastercard Foundation outreach (Mkulima angle)
  - These are 12–18 month conversations — start early
  - Frame: Mkulima Sustain as agricultural finance infrastructure for smallholder farmers
  - Initial ask: not funding — relationship building and pilot programme interest

  > **[DISPATCH]** "Draft a one-page executive summary of the Mkulima Sustain component of Sustena, suitable for an initial outreach email to IFC (International Finance Corporation) or AGRA (Alliance for a Green Revolution in Africa). The summary should cover: (1) problem: smallholder farmers in Kenya lack real-time market price intelligence and cannot optimally time their produce sales, (2) Sustena's solution: the Mkulima Sustain — an AI farm manager that tracks crop cycles, receives buyer purchase orders (from Vyyb and other Biashara sustains), broadcasts supply signals, and manages farm finances through M-Pesa, (3) the data advantage: as Mkulima sustains scale, Sustena builds the most detailed, real-time agricultural supply chain dataset in East Africa — valuable to researchers, DFIs, and governments, (4) ask: we are seeking to discuss a potential pilot programme with 50–100 smallholder farmers in Nairobi's supply catchment area. One page, investor-quality English."

---

### Epic 1.3 — Operations and Metrics

- [ ] 🔴 **1.3.1** Weekly metrics dashboard (manual, Google Sheets)
  - Metrics to track: MAU, new signups/week, 30-day retention rate, MRR, churn rate, NPS (monthly survey)
  - Update every Monday; share with Bonnie; review together Friday

- [ ] 🔴 **1.3.2** Customer support process
  - WhatsApp group for beta users: "Sustena Beta Feedback"
  - Brian manages all support queries (Phase 0–1); respond within 24h
  - Log all issues in a shared Google Sheet: issue, user, date, resolution, is-product-bug (yes/no)
  - Weekly: send product bug list to Bonnie; send user testimonials list for marketing

- [ ] 🟡 **1.3.3** User onboarding SLA
  - Every new user who signs up via WhatsApp gets a personal WhatsApp message from Brian within 2 hours
  - Message: "Karibu Sustena! Mimi ni Brian, moja wa waanzilishi. Nataka kukusaidia uanze vizuri. Je, unaanza kwa Homestead (fedha za nyumbani) au Biashara?"
  - Personal touch in Phase 0–1 is a competitive advantage over automated SaaS onboarding

---

### Epic 1.4 — Investor Readiness

- [ ] 🟡 **1.4.1** Investor deck (first version)
  - 10–12 slides; Claude Design for visuals; Brian for content

  > **[DISPATCH]** "Write the narrative content for a 12-slide seed investor pitch deck for Sustena Ltd. Slide titles and content: (1) Cover: 'Sustena — Intelligence infrastructure for complex African life' + brief tagline. (2) Problem: The cost of complexity for Kenyan households and businesses (manual management, poor financial decisions, no systematic planning). (3) Solution: Orchie — the AI delegate that manages your sustain (in plain language, no jargon). (4) Product demo: 3 use cases — Homestead budget ring, Chama contribution tracking, Biashara daily revenue summary. (5) Market: Kenya's 50M population, 3.5M MSMEs, 300,000+ chamas — and East Africa as the wider TAM. (6) Traction: [leave blank — insert real numbers]. (7) Business model: subscription tiers + chama group rates + Biashara SaaS + data licensing (Phase 2) + pawa token network (Phase 3). (8) Technology: brief on Sustena primitives, Mycelium, web3 roadmap — 3 bullets max. (9) Team: Bonventure (systems architect, [background]) + Brian (finance lead, [background]) + Kang'iri (strategic equity). (10) Financials: 12-month projection + funding ask ($350,000) + use of funds. (11) Roadmap: Phase 0 (WhatsApp, now) → Phase 1 (platform, Month 3) → Phase 2 (mobile app, Month 6) → Phase 3 (web3, Month 12). (12) Ask: $350,000 seed round, 15–20% dilution. Write the narrative (speaker notes + key point per slide), not the full slide text."

  > **[DESIGN]** "Design a 12-slide pitch deck for Sustena Ltd following the brand identity system (dark navy background, amber accents, mycelium green, Space Grotesk typography). The deck should feel like: Stripe's clean precision + African warmth + The Expanse's data-forward aesthetic. Each slide has: one headline (large, amber), supporting data or visual, minimal text. Slide 3 (Solution) should include a mockup of the Orchie WhatsApp chat interface. Slide 8 (Technology) should include a simplified diagram of the Sustena primitive layers (State → Operators → Operatives → Sustain → Council). Slide 10 (Financials) should include the 12-month MRR curve chart. Deliver as Keynote or PowerPoint."

- [ ] 🟡 **1.4.2** Investor pipeline
  - Tier 1 targets (East Africa tech VCs): DOB Equity, Novastar, Seedstars Africa, AfricArena
  - Tier 2 (impact investors): Acumen Fund, Omidyar Network, Mastercard Foundation Ventures
  - Tier 3 (angels): Kenyan tech founders (past and present), diaspora investors
  - Action: send cold email + deck to 5 Tier 1 investors in Month 2; iterate deck based on feedback before going wider

  > **[DISPATCH]** "Write a cold outreach email to Novastar Ventures, an East Africa-focused VC fund. Sustena is raising a $350,000 seed round. The email should: (1) open with our traction (insert real numbers when available — write placeholder), (2) explain Sustena in one sentence: 'We are building the AI operating layer for Kenyan households, chamas, and MSMEs — delivered via WhatsApp today and a full platform by Q3 2026.' (3) briefly note the market (Kenya's 50M people, 3.5M MSMEs, WhatsApp penetration >90%), (4) explain why Novastar specifically (their portfolio focus on financial inclusion and digital infrastructure in East Africa), (5) ask for a 20-minute call. Keep email under 250 words. No attachments on first email — offer to share deck on reply."

---

## Phase 2 — Platformisation (Months 3–6)

### Epic 2.1 — Sales and Revenue Scale

- [ ] 🟡 **2.1.1** Formalise pricing and payment infrastructure
  - Integrate M-Pesa Daraja for subscription payments (Bonnie implements; Brian specifies the flow)
  - Pricing page on sustena.io: Personal (KES 150/month), Chama (KES 2,000/month), Biashara (KES 1,000/month)
  - Invoicing: monthly automatic via M-Pesa STK push; failed payment → 3-day grace → account suspended

- [ ] 🟡 **2.1.2** Mkulima Premium launch — 500 farmers by Month 6
  - Partnership with agricultural extension officers: they introduce Sustena to their farmer networks
  - Pilot: 50 farmers in one value chain (tomatoes: Nairobi supply catchment)
  - Onboarding: Brian visits the area, does in-person WhatsApp setup with 5 farmers; they train the rest

- [ ] 🟡 **2.1.3** B2B pipeline — food buyer network
  - Target: restaurant chains, supermarkets (Naivas, Quickmart), hotel procurement teams
  - Pitch: Vyyb's Mkulima integration gives them pre-harvest procurement at below-market prices
  - Business model: 2% commission on each PO facilitated through the platform

- [ ] 🔴 **2.1.4** Hiring — first non-founder team members
  - Target Month 4 (post-seed): Customer Success Manager (KES 80,000–120,000/month)
  - Target Month 6: Marketing Associate (KES 60,000–80,000/month)
  - Write job descriptions; post on LinkedIn, BrighterMonday, Shortlist; interview and onboard

  > **[DISPATCH]** "Write a job description for a Customer Success Manager at Sustena Ltd. The role is the first non-founder hire. Responsibilities: (1) manage WhatsApp-based customer support for 1,000+ users, (2) conduct user onboarding calls for chama and Biashara tier clients, (3) collect and synthesise product feedback into a weekly brief for the founders, (4) own the metrics: 30-day retention rate, NPS, churn rate. Requirements: 2+ years customer success or support experience, excellent written and verbal Swahili + English, familiarity with M-Pesa and basic financial concepts, WhatsApp Business experience. Nice to have: experience with SaaS or fintech products. Salary range: KES 80,000–120,000/month + equity (0.25–0.5% vesting over 4 years). Location: Nairobi (hybrid). Tone: warm, mission-driven, honest about the startup stage."

---

### Epic 2.2 — Brand and Marketing at Scale

- [ ] 🟡 **2.2.1** Sustena blog — milestone reporting
  - Post monthly: platform stats (MAU, operatives active, operators executed, pawa consumed), new sustain templates, community events
  - These posts become investor update material and proof of momentum

  > **[DISPATCH]** "Write a template for Sustena's monthly milestone blog post. The post should be factual and honest — not a marketing spin exercise. Structure: (1) header with month and key headline metric (e.g., 'May 2026: 312 active sustains, 4,200 Orchie conversations'), (2) 3 metrics: MAU, new sustain templates published, total operators executed, (3) one user story (anonymised): a real user describes how Orchie helped them this month, (4) product update: what shipped this month (2–3 bullet points), (5) what's coming next month (2–3 bullet points), (6) event calendar: any upcoming workshops, webinars, or community calls + sign-up link. Length: 400–500 words. Tone: transparent, data-forward, community-first."

- [ ] 🟡 **2.2.2** Community events — quarterly Sustena meetups
  - Nairobi-based: invite beta users, chamas, Biashara clients, developers
  - Format: 2-hour event — product demo (30min), panel Q&A with users (30min), networking (60min)
  - First event: target Month 3 (post first 100 users)

- [ ] 🟢 **2.2.3** Developer community — Mycelium contributor programme
  - Target: Kenyan tech community (iHub, Nairobi Dev Community, Stack Overflow KE)
  - Launch: Sustena Developer Programme — free access to the platform + monthly office hours with Bonnie
  - Incentive: first operative published to Mycelium Library = 1,000 pawa welcome grant

---

### Epic 2.3 — Regulatory and Compliance Milestones

- [ ] 🟡 **2.3.1** CBK Digital Credit Provider licence preparation
  - Start the process in Month 4 (Phase 2) — takes 12–18 months
  - Requirements: KES 20M+ capital in reserve (must be in bank), robust KYC/AML system, audited financials
  - Brian's role: manage the lawyer relationship, maintain the capital reserve account, track application status

  > **[DISPATCH]** "Create a CBK Digital Credit Provider (DCP) licence preparation checklist for Colosso Finance Ltd. Cover the key requirements under the Central Bank of Kenya (Amendment) Act 2021 (Digital Credit Providers Regulations 2022): (1) capital requirements (KES 20M minimum), (2) fit and proper test for directors, (3) data governance requirements, (4) interest rate cap compliance, (5) KYC/AML requirements, (6) application documents required. For each requirement: what it is, what we need to prepare, estimated timeline. Note which requirements Sustena's ODPC registration and existing data governance policies already satisfy. This is an internal planning document — not legal advice."

- [ ] 🟡 **2.3.2** CMA Sandbox application (for pawa/STC token framework)
  - File in Month 6–9 — after Phase 1 live data demonstrates utility
  - Frame pawa tokens as loyalty points (not securities) in Phase 1; ask for sandbox guidance on Phase 3 STC

---

## Phase 3 — Decentralisation (Month 12+)

### Epic 3.1 — STC Launch and Token Economy

- [ ] 🟢 **3.1.1** STC tokenomics document
  - Co-write with Bonnie: Brian owns the economic model; Bonnie owns the technical implementation
  - Sections: token allocation, vesting schedule, conversion rate methodology, governance rights, use of treasury

  > **[DISPATCH]** "Draft a Sustena Token (STC) tokenomics document for internal review. STC is a governance token (not a security — explicitly disclaim). Sections needed: (1) Purpose: STC gives holders voting rights on Mycelium governance parameters (operator registration fees, royalty split percentages, data ethics policies). (2) Supply: fixed cap of 100,000,000 STC. (3) Allocation: [placeholder — suggest typical allocations for: team/founders, community contributors, ecosystem fund, treasury, public sale]. (4) Vesting: [suggest appropriate vesting for each category]. (5) Pawa-to-STC conversion: describe the conversion mechanism (DAO-governed rate, conversion windows). (6) Governance rights: what STC holders can vote on, quorum requirements, timelock. (7) What STC is NOT: not a dividend-bearing instrument, not a profit share, not a claim on company assets. Note: this document is subject to legal review for Kenya CMA Sandbox compliance."

- [ ] 🟢 **3.1.2** VASP (Virtual Asset Service Provider) partnership for KES off-ramp
  - Research and shortlist Kenyan VASPs with CBK recognition
  - Likely candidates: Binance P2P Kenya, Paxful, or a local fintech with VASP registration
  - Brian leads commercial negotiation; Bonnie handles technical API integration

---

## Quick Reference — Brian's Weekly Routine

**Monday:**
- Update metrics dashboard (MAU, MRR, churn, NPS)
- Send weekly investor update (once fundraising starts)
- Review product bug list from support log

**Wednesday:**
- Conduct 2–3 user calls (chama chairpersons or Biashara owners)
- Review partnership pipeline; send follow-up emails

**Friday:**
- Brief Bonnie on week's user feedback (30-minute sync)
- Write Substack draft (if it's a Substack week)
- Update fundraising tracker

**Monthly:**
- Draft and publish Sustena blog milestone post
- Financial reconciliation (NCBA statement vs. expense tracker)
- Investor update (if in fundraising mode)
- ODPC data processing register update

---

*End of Brian Master Roadmap v1.0*
*Next update: add Phase 2 Mkulima field operations subtasks after pilot programme is designed.*
