'use strict';
// Input-range weighted frequencies over reachable supported exact combinations.
function aggregate(group, weights, reports, actionCount) {
  const sums=Array(actionCount).fill(0); let weight=0;
  for(const i of group){const r=reports[i],w=weights[i]; if(!(w>0)||!r||!r.accumulated||!r.evs.some(v=>v!==null))continue;weight+=w;r.frequencies.forEach((f,a)=>sums[a]+=w*f)}
  return {weight, frequencies:weight?sums.map(x=>x/weight):null};
}
if(typeof module!=='undefined') module.exports={aggregate};
