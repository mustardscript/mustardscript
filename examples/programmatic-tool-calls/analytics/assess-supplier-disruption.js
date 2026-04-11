/*
Inputs:
  - region: string
  - scenario: string

Capabilities:
  - list_suppliers(region) -> [{ supplierId, name, tier, region, riskEvent, skuIds: string[], recoveryDays }]
  - fetch_inventory_positions(skuIds) -> [{ sku, onHandUnits, daysOfCover, inboundUnits }]
  - fetch_open_shipments(supplierIds) -> [{ supplierId, shipmentId, status, delayedDays }]
  - lookup_alternate_sources(skuIds) -> [{ sku, alternateSupplier, qualified, leadTimeDays, unitCostDelta }]
  - map_sku_revenue(skuIds) -> [{ sku, weeklyRevenue, criticality }]
*/

async function assessSupplierDisruption() {
  const suppliers = await list_suppliers(region);
  const supplierIds = [];
  const skuSet = new Set();

  for (const supplier of suppliers) {
    supplierIds.push(supplier.supplierId);
    for (const sku of supplier.skuIds) {
      skuSet.add(sku);
    }
  }

  const skuIds = Array.from(skuSet);
  const [inventory, shipments, alternates, skuRevenue] = await Promise.all([
    fetch_inventory_positions(skuIds),
    fetch_open_shipments(supplierIds),
    lookup_alternate_sources(skuIds),
    map_sku_revenue(skuIds),
  ]);

  const inventoryBySku = new Map();
  for (const item of inventory) {
    inventoryBySku.set(item.sku, item);
  }

  const revenueBySku = new Map();
  for (const item of skuRevenue) {
    revenueBySku.set(item.sku, item);
  }

  const alternatesBySku = new Map();
  for (const alternate of alternates) {
    const bucket = alternatesBySku.get(alternate.sku) ?? [];
    bucket.push(alternate);
    alternatesBySku.set(alternate.sku, bucket);
  }

  const delayedShipments = new Map();
  for (const shipment of shipments) {
    if (shipment.status === "delayed" || shipment.delayedDays > 0) {
      delayedShipments.set(
        shipment.supplierId,
        (delayedShipments.get(shipment.supplierId) ?? 0) + 1,
      );
    }
  }

  const impactedSuppliers = [];
  let weeklyRevenueAtRisk = 0;

  for (const supplier of suppliers) {
    const skuImpacts = [];
    for (const sku of supplier.skuIds) {
      const stock = inventoryBySku.get(sku);
      const revenue = revenueBySku.get(sku);
      const alternateOptions = alternatesBySku.get(sku) ?? [];

      const hasQualifiedAlternate = alternateOptions.some(
        (option) => option.qualified && option.leadTimeDays <= supplier.recoveryDays,
      );

      const constrained =
        !stock ||
        stock.daysOfCover < supplier.recoveryDays ||
        (!hasQualifiedAlternate && (revenue?.criticality === "high" || stock.daysOfCover < 21));

      if (constrained) {
        weeklyRevenueAtRisk += revenue?.weeklyRevenue ?? 0;
        skuImpacts.push({
          sku,
          daysOfCover: stock?.daysOfCover ?? 0,
          weeklyRevenue: revenue?.weeklyRevenue ?? 0,
          criticality: revenue?.criticality ?? "unknown",
          alternateOptions,
        });
      }
    }

    if (skuImpacts.length > 0) {
      impactedSuppliers.push({
        supplierId: supplier.supplierId,
        supplierName: supplier.name,
        tier: supplier.tier,
        riskEvent: supplier.riskEvent,
        recoveryDays: supplier.recoveryDays,
        delayedShipmentCount: delayedShipments.get(supplier.supplierId) ?? 0,
        skuImpacts,
      });
    }
  }

  let highestRiskSupplier = null;
  for (const supplier of impactedSuppliers) {
    if (
      !highestRiskSupplier ||
      supplier.skuImpacts.length > highestRiskSupplier.skuImpacts.length
    ) {
      highestRiskSupplier = supplier;
    }
  }

  const recommendations = [];
  if (weeklyRevenueAtRisk > 500000) {
    recommendations.push("activate_executive_supply_review");
  }
  if (impactedSuppliers.some((supplier) => supplier.tier === 1)) {
    recommendations.push("prebook_qualified_alternates");
  }
  if (impactedSuppliers.some((supplier) => supplier.delayedShipmentCount > 0)) {
    recommendations.push("expedite_inbound_shipments");
  }

  return {
    region,
    scenario,
    supplierCount: suppliers.length,
    impactedSupplierCount: impactedSuppliers.length,
    weeklyRevenueAtRisk,
    highestRiskSupplier:
      highestRiskSupplier === null
        ? null
        : {
            supplierId: highestRiskSupplier.supplierId,
            supplierName: highestRiskSupplier.supplierName,
            skuImpactCount: highestRiskSupplier.skuImpacts.length,
          },
    recommendations,
    impactedSuppliers,
  };
}

assessSupplierDisruption();
