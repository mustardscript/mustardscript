const initial = new Date(1234);
const cloned = new Date(initial);

[initial.getTime(), cloned.getTime(), new Date(-50).getTime()];
