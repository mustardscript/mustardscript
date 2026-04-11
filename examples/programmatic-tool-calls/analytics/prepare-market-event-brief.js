/*
Inputs:
  - symbol: string
  - eventType: string
  - noteWindowDays: number

Capabilities:
  - fetch_event_calendar(symbol, eventType) -> { nextEventDate, blackoutStart, timezone }
  - fetch_consensus_estimates(symbol) -> { revenue, eps, revisions: [{ broker, direction, deltaPct }] }
  - fetch_transcript_history(symbol, limit) -> [{ quarter, excerpts: string[] }]
  - fetch_options_positioning(symbol) -> { impliedMovePct, putCallRatio, skew, largestStrikes: [{ strike, side, openInterest }] }
  - search_research_notes(symbol, query) -> [{ source, publishedAt, excerpt }]
*/

async function prepareMarketEventBrief() {
  const [calendar, consensus, transcripts, positioning, researchNotes] = await Promise.all([
    fetch_event_calendar(symbol, eventType),
    fetch_consensus_estimates(symbol),
    fetch_transcript_history(symbol, 4),
    fetch_options_positioning(symbol),
    search_research_notes(symbol, "pricing demand inventory margin guide"),
  ]);

  const signalCounts = new Map();
  const quarterSignals = [];

  for (const transcript of transcripts) {
    const matchedSignals = [];
    for (const excerpt of transcript.excerpts) {
      for (const match of excerpt
        .toLowerCase()
        .matchAll(/\b(pricing|demand|inventory|margin|headwind|guide|capacity)\b/g)) {
        const token = match[1];
        signalCounts.set(token, (signalCounts.get(token) ?? 0) + 1);
        matchedSignals.push(token);
      }
    }
    quarterSignals.push({
      quarter: transcript.quarter,
      matchedSignals,
    });
  }

  let upwardRevisions = 0;
  let downwardRevisions = 0;
  for (const revision of consensus.revisions) {
    if (revision.direction === "up") {
      upwardRevisions += 1;
    } else if (revision.direction === "down") {
      downwardRevisions += 1;
    }
  }

  const noteThemes = [];
  for (const note of researchNotes) {
    const normalized = note.excerpt
      .toLowerCase()
      .replaceAll(/\s+/g, " ")
      .replaceAll(/[^\w ]+/g, " ");
    if (normalized.includes("margin pressure")) {
      noteThemes.push("margin_pressure");
    }
    if (normalized.includes("demand stabilization")) {
      noteThemes.push("demand_stabilization");
    }
    if (normalized.includes("inventory digestion")) {
      noteThemes.push("inventory_digestion");
    }
  }

  return {
    symbol,
    eventType,
    eventWindow: {
      nextEventDate: calendar.nextEventDate,
      blackoutStart: calendar.blackoutStart,
      timezone: calendar.timezone,
    },
    consensus,
    revisionBalance: upwardRevisions - downwardRevisions,
    optionsPositioning: positioning,
    noteThemes,
    transcriptSignalFrequency: Object.fromEntries(signalCounts),
    quarterSignals,
    keyQuestions: [
      "Is pricing holding without incremental discounting?",
      "Does inventory digestion extend beyond the current quarter?",
      "Are guide assumptions consistent with options-implied volatility?",
    ],
  };
}

prepareMarketEventBrief();
