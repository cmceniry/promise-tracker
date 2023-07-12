import { Resolution, Offer } from "./resolution.js";

describe("Resolution/Offer conversions", () => {
  it('toObject should handle arbitrary depth', () => {
    const b3 = new Resolution("b3");
    b3.addSatisfied("c3");
    expect(b3.toObject()).toEqual(
      {behavior: "b3", satisfied: [
        {component: "c3"},
      ]},
    );
    const b2 = new Resolution("b2");
    b2.addSatisfied("c2", [b3]);
    const b1 = new Resolution("b1");
    b1.addSatisfied("c1", [b2]);
    expect(b1.toObject()).toEqual(
      {behavior: "b1", satisfied: [
        {component: "c1", conditions: [
          {behavior: "b2", satisfied: [
            {component: "c2", conditions: [
              {behavior: "b3", satisfied: [
                {component: "c3"},
              ]},
            ]},
          ]},
        ]},
      ]},
    );
  });

  it('should collapse a resolution two levels', () => {
  });
});

describe("Resolution/Offer collapsing", () => {
  it('should not collapse one item', () => {
    const a = Resolution.fromObject(
      {behavior: "b1", satisfied: [
        {component: "c1"},
      ]},
    );
    expect(a.collapse().toObject()).toEqual(a.toObject());
  });

  it('should not collapse nonmatching', () => {
    const a = Resolution.fromObject(
      {behavior: "b1", satisfied: [
        {component: "c1", conditions: [
          {behavior: "b2", satisfied: [
            {component: "c2", conditions: [
              {behavior: "b3", satisfied: [
                {component: "c3"},
              ]},
            ]},
          ]},
        ]},
      ]},
    );
    expect(a.collapse().toObject()).toEqual(a.toObject());
  });

  it('should collapse 2S', () => {
    const a = Resolution.fromObject(
      {behavior: "b1", satisfied: [
        {component: "c1", conditions: [
          {behavior: "b2", satisfied: [
            {component: "c1"},
          ]},
        ]},
      ]},
    );
    expect(a.collapse().toObject()).toEqual(
      {behavior: "b1", satisfied: [
        {component: "c1"},
      ]},
    );
  });

  it('should collapse 3S', () => {
    const a = Resolution.fromObject(
      {behavior: "b1", satisfied: [
        {component: "c1", conditions: [
          {behavior: "b2", satisfied: [
            {component: "c1", conditions: [
              {behavior: "b3", satisfied: [
                {component: "c3"},
              ]},
            ]},
          ]},
        ]},
      ]},
    );
    expect(a.collapse().toObject()).toEqual(
      {behavior: "b1", satisfied: [
        {component: "c1", conditions: [
          {behavior: "b3", satisfied: [
            {component: "c3"},
          ]},
        ]},
      ]},
    );
  });

  it('should collapse 10S', () => {
    var a = {behavior: "b10", satisfied: [{component: "c"}]};
    for (var i = 9; i > 0; i--) {
      a = {behavior: "b" + i, satisfied: [{component: "c", conditions: [a]}]};
    };
    expect(Resolution.fromObject(a).collapse().toObject()).toEqual({behavior: "b1", satisfied: [{component: "c"}]});
  });

  it('should collapse splits', () => {
    const a = Resolution.fromObject(
      {behavior: "b1", satisfied: [
        {component: "c1", conditions: [
          {behavior: "b2a", satisfied: [
            {component: "c1", conditions: [
              {behavior: "b3a", satisfied: [
                {component: "c3"},
              ]},
            ]},
          ]},
          {behavior: "b2b", satisfied: [
            {component: "c1", conditions: [
              {behavior: "b3b", satisfied: [
                {component: "c3"},
              ]},
            ]},
          ]},
        ]},
      ]},
    );
    expect(a.collapse().toObject()).toEqual(
      {behavior: "b1", satisfied: [
        {component: "c1", conditions: [
          {behavior: "b3a", satisfied: [
            {component: "c3"},
          ]},
          {behavior: "b3b", satisfied: [
            {component: "c3"},
          ]},
        ]},
      ]},
    );
  });

  it('should collapse splits with unsatisfied', () => {
    const a = Resolution.fromObject(
      {behavior: "b1", unsatisfied: [
        {component: "c1", conditions: [
          {behavior: "b2", unsatisfied: [
            {component: "c1", conditions: [
              {behavior: "b3", unsatisfied: [
              ]},
            ]},
          ]},
        ]},
      ]},
    );
    expect(a.collapse().toObject()).toEqual(
      {behavior: "b1", unsatisfied: [
        {component: "c1", conditions: [
          {behavior: "b3", unsatisfied: [
          ]},
        ]},
      ]},
    );
  });
});