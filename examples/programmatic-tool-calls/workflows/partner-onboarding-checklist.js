/*
Inputs:
  - partnerId: string

Capabilities:
  - fetch_partner_record(partnerId)
  - fetch_required_documents(partnerType)
  - list_uploaded_documents(partnerId)
*/

async function main() {
  const partner = await fetch_partner_record(partnerId);
  const required = await fetch_required_documents(partner.type);
  const uploaded = await list_uploaded_documents(partnerId);
  const missing = [];

  for (const name of required) {
    if (!uploaded.includes(name)) {
      missing.push(name);
    }
  }

  const output = {};
  output.partnerId = partnerId;
  output.partnerType = partner.type;
  output.missingDocuments = missing;
  return output;
}

main();
