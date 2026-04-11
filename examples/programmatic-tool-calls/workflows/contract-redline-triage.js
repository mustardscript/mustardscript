/*
Inputs:
  - contractId: string

Capabilities:
  - fetch_contract_text(contractId)
  - fetch_clause_library()
*/

async function main() {
  const contract = await fetch_contract_text(contractId);
  const library = await fetch_clause_library();
  const mentions = [];

  for (const clause of library) {
    if (contract.body.toLowerCase().includes(clause.term.toLowerCase())) {
      mentions.push(clause.term);
    }
  }

  const sanitized = contract.body.replace(/\s+/g, " ").trim();
  const output = {};
  output.contractId = contractId;
  output.mentions = mentions;
  output.preview = sanitized.slice(0, 160);
  return output;
}

main();
