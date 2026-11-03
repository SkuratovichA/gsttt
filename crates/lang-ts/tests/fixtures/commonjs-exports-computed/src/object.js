const helper = () => 1;
const other = () => 2;
module.exports = run;
function run() {}
module.exports[Symbol.for('x')] = { helper, other, plain: 3 };
