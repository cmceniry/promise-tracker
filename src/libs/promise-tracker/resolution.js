export class Resolution {
  constructor(behavior, satisfied, unsatisfied) {
    this.behavior = behavior;
    this.satisfied = [];
    this.unsatisfied = [];
  }

  static fromObject(obj) {
    const ret = new Resolution(obj.behavior);
    if (obj.satisfied) { ret.satisfied = obj.satisfied.map((s) => {return Offer.fromObject(s)}) };
    if (obj.unsatisfied) { ret.unsatisfied = obj.unsatisfied.map((u) => {return Offer.fromObject(u)}) };
    return ret;
  }

  addSatisfied(componentName, conditions = null) {
    this.satisfied.push(new Offer(componentName, conditions));
  }

  addUnsatisfied(componentName, conditions) {
    this.unsatisfied.push(new Offer(componentName, conditions));
  }

  isSatisfied() {
    return this.satisfied.length > 0;
  }

  prune() {
    const ret = new Resolution(this.behavior);
    ret.satisfied = this.satisfied.map((s) => {return s.prune()});
    if (this.isSatisfied()) {
      return ret
    }
    ret.unsatisfied = this.unsatisfied.map((u) => {return u.prune()});
    return ret
  }

  toObject() {
    const ret = {behavior: this.behavior};
    if (this.satisfied.length > 0) { ret.satisfied = this.satisfied.map((s) => {return s.toObject()}) }
    if (this.unsatisfied.length > 0) { ret.unsatisfied = this.unsatisfied.map((u) => {return u.toObject()}) }
    if (this.satisfied.length === 0 && this.unsatisfied.length === 0) { ret.unsatisfied = [] }
    return ret;
  }
}

export class Offer {
  constructor(componentName, conditions = null) {
    this.componentName = componentName;
    this.conditions = conditions;
  }

  static fromObject(obj) {
    const ret = new Offer(obj.component);
    if (obj.conditions) { ret.conditions = obj.conditions.map((c) => {return Resolution.fromObject(c)}) }
    return ret;
  }

  prune() {
    if (!this.conditions) {
      return new Offer(this.componentName);
    }
    return new Offer(this.componentName, this.conditions.map((c) => c.prune()));
  }

  toObject() {
    if (!this.conditions) {
      return {component: this.componentName};
    }
    return {
      component: this.componentName,
      conditions: this.conditions.map((c) => c.toObject()),
    };    
  }
}
