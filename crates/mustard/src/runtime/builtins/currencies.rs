// Derived from Unicode CLDR 48.2.1, commit 26a79cb42bfcc90def764102aa2af126d9ef3108.
// cldr-core/supplemental/currencyData.json: fractions._digits (not cash digits).
// cldr-numbers-full/main/en/currencies.json: symbol, with CLDR currency spacing.
// Unicode-3.0 license: see crates/mustard/UNICODE-LICENSE and root UNICODE-LICENSE.

pub(super) fn currency_digits(code: &str) -> usize {
    match code {
        "ADP" | "AFN" | "ALL" | "BIF" | "BYR" | "CLP" | "COP" | "DJF" | "ESP" | "GNF" | "HUF"
        | "IDR" | "IQD" | "IRR" | "ISK" | "ITL" | "JPY" | "KMF" | "KPW" | "KRW" | "LAK" | "LBP"
        | "LUF" | "MGA" | "MGF" | "MMK" | "MRO" | "PKR" | "PYG" | "RWF" | "SLL" | "SOS" | "STD"
        | "SYP" | "TMM" | "TRL" | "UGX" | "UYI" | "VND" | "VUV" | "XAF" | "XOF" | "XPF" | "YER"
        | "ZMK" | "ZWD" => 0,
        "BHD" | "JOD" | "KWD" | "LYD" | "OMR" | "TND" => 3,
        "CLF" | "UYW" => 4,
        _ => 2,
    }
}

pub(super) fn currency_symbol(code: &str) -> (&str, bool) {
    match code {
        "AUD" => ("A$", false),
        "BRL" => ("R$", false),
        "CAD" => ("CA$", false),
        "CNY" => ("CN¥", false),
        "EUR" => ("€", false),
        "GBP" => ("£", false),
        "HKD" => ("HK$", false),
        "ILS" => ("₪", false),
        "INR" => ("₹", false),
        "JPY" => ("¥", false),
        "KRW" => ("₩", false),
        "MXN" => ("MX$", false),
        "NZD" => ("NZ$", false),
        "PHP" => ("₱", false),
        "TWD" => ("NT$", false),
        "USD" => ("$", false),
        "VND" => ("₫", false),
        "XAF" => ("FCFA", true),
        "XCD" => ("EC$", false),
        "XCG" => ("Cg.", true),
        "XOF" => ("F CFA", true),
        "XPF" => ("CFPF", true),
        "XXX" => ("¤", false),
        _ => (code, true),
    }
}
