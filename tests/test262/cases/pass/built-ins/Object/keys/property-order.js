const object = {};
object.beta = 2;
object[10] = 10;
object.alpha = 1;
object[2] = 3;
object["01"] = 4;
[Object.keys(object), JSON.stringify(object)];
