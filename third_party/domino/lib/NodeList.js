"use strict";

var NodeList;

try {
    // Attempt to use ES6-style Array subclass if possible.
    NodeList = require('./NodeList.es6.js');
} catch (e) {
    // No support for subclassing array, return an actual Array object.
    NodeList = require('./NodeList.es5.js');
}

// Static snapshot. querySelectorAll must not return a live HTMLCollection:
// those have .item but are not iterable, and the DOM spec wants a static list.
NodeList.snapshot = function(nodes) {
    var list = [];
    var n = nodes && typeof nodes.length === 'number' ? nodes.length : 0;
    for (var i = 0; i < n; i++) {
        var item = nodes[i];
        if ((item === undefined || item === null) && typeof nodes.item === 'function') {
            item = nodes.item(i);
        }
        if (item != null) list.push(item);
    }
    return new NodeList(list);
};

module.exports = NodeList;
