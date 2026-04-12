const values = [1, 2];
values["01"] = 7;
values[4294967295] = 9;
values.tail = 4;
[Object.keys(values), Object.entries(values), JSON.stringify(values)];
