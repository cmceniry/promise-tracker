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
    subgraph "c2"
      edbab45572c72a5d9440b40bcc0500c0["b1"]
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
    subgraph "c2"
      edbab45572c72a5d9440b40bcc0500c0["b1"]
    end
    subgraph "c3"
      fbfba2e45c2045dc5cab22a5afe83d9d["b2"]
    end
    subgraph "c4"
      7a6f150b83091ce20c89368641f9a137["b3"]
    end
    7a6f150b83091ce20c89368641f9a137 --> edbab45572c72a5d9440b40bcc0500c0
    fbfba2e45c2045dc5cab22a5afe83d9d --> edbab45572c72a5d9440b40bcc0500c0`);
  });

});
