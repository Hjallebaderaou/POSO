const {test}=require('node:test');
const assert=require('node:assert/strict');
const {aggregate}=require('./matrix');
test('matrix uses input weights and excludes blocked, unsupported and unreachable combos',()=>{
  const reports=[{frequencies:[1,0],evs:[0,0],accumulated:true},{frequencies:[0,1],evs:[1,2],accumulated:true},null,{frequencies:[1,0],evs:[null,null],accumulated:true},{frequencies:[1,0],evs:[2,3],accumulated:false}];
  assert.deepEqual(aggregate([0,1,2,3,4],[1,3,99,99,99],reports,2),{weight:4,frequencies:[0.25,0.75]});
  assert.deepEqual(aggregate([2,3,4],[1,1,1,1,1],reports,2),{weight:0,frequencies:null});
});
