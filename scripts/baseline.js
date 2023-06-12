import PromiseTracker from '../src/libs/promise-tracker/promise-tracker.js';
import { Component } from '../src/libs/promise-tracker/contract.js';

const pt = new PromiseTracker();
pt.addComponent(new Component('bar'));
console.log(pt.getComponentNames());