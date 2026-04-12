let missing;
missing ??= 7;
const box = { present: 5, absent: undefined };
box.present ??= 9;
box.absent ??= 11;
const key = 'dynamic';
box[key] ??= 13;
[missing, box.present, box.absent, box.dynamic];
