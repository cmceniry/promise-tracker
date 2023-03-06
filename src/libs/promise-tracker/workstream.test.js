import workstream from './workstream.js';

describe('rendering', () => {

  it('does a simple good flowchart', () => {
    const input = {
      component: "c1",
      behavior: "b1",
      satisfied: [
        {
          component: "c2",
        }
      ],
    };
    const r = workstream(input);
    expect(r).toEqual(`flowchart TD
    subgraph c2
      b1
    end`);
  });

  it(`does a simple good nested flowchart`, () => {
    const input = {
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
            {
              behavior: "b3",
              satisfied: [
                {
                  component: "c4",
                },
              ],
            },
          ],
        },
      ],
    };
    const r = workstream(input);
    expect(r).toEqual(`flowchart TD
    subgraph c2
      b1
    end
    subgraph c3
      b2
    end
    subgraph c4
      b3
    end
    b2 ->> b1
    b3 ->> b1`);
  });

});
