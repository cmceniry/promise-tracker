import diagram from './diagram.js';

describe('rendering', () => {

  it('does a simple good diagram', () => {
    const input = {
      component: "c1",
      behavior: "b1",
      satisfied: [
        {
          component: "c2",
        }
      ],
    };
    const r = diagram(input);
    expect(r).toEqual(`sequenceDiagram
    rect rgb(0,255,0)
        c1 ->> c2: b1
    end`);
  });

  it('does a nested good diagram', () => {
    const input = {
      component: "c1",
      behavior: "b1",
      satisfied: [
        {
          component: "c2",
          conditions: [
            {
              behavior: "b2",
              satisfied: [
                {
                  component: "c3",
                },
              ],
            },
          ],
        },
      ],
    }
    const r = diagram(input);
    expect(r).toEqual(`sequenceDiagram
    rect rgb(0,255,0)
        c1 ->> c2: b1
        rect rgb(0,255,0)
            c2 ->> c3: b2
        end
    end`)
  });

  it('does a simple bad diagram', () => {
    const r = diagram({
      component: "c1",
      behavior: "b1",
      unsatisfied: [],
    });
    expect(r).toEqual(`sequenceDiagram
    rect rgb(255,0,0)
        c1 -X c1: b1
    end`);
  });

  it('does a nested bad diagram', () => {
    const r = diagram({
      component: "c1",
      behavior: "b1",
      unsatisfied: [
        {
          component: "c2",
          conditions: [
            {
              behavior: "b2",
              unsatisfied: [],
            },
          ],
        },
      ],
    });
    expect(r).toEqual(`sequenceDiagram
    rect rgb(255,0,0)
        c1 ->> c2: b1
        rect rgb(255,0,0)
            c2 -X c2: b2
        end
    end`);
  })

  it("nesting works 1", () => {
    const r = diagram({
      component: "c1",
      behavior: "b1",
      satisfied: [
        {
          component: "c2",
        },
      ],
      unsatisfied: [],
    })
    expect(r).toEqual(`sequenceDiagram
    rect rgb(0,255,0)
        c1 ->> c2: b1
    end
    rect rgb(255,0,0)
        c1 -X c1: b1
    end`);
  });

  it("nesting works", () => {
    const r = diagram({
      component: "c1",
      behavior: "b1",
      unsatisfied: [
        {
          component: "c2",
          conditions: [
            {
              behavior: "b2b",
              satisfied: [
                {
                  component: "c2b"
                }
              ]
            },
            {
              behavior: "b2a",
              unsatisfied: [],
            },
          ],
        },
      ],
    });
    expect(r).toEqual(`sequenceDiagram
    rect rgb(255,0,0)
        c1 ->> c2: b1
        rect rgb(0,255,0)
            c2 ->> c2b: b2b
        end
        rect rgb(255,0,0)
            c2 -X c2: b2a
        end
    end`);
  });

});
