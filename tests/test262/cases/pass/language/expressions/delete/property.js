const obj = { value: 1 };
const arr = [1, 2];
[delete obj.value, "value" in obj, delete arr[0], arr.length, 0 in arr, JSON.stringify(arr)];
