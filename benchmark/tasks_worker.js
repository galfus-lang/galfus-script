let inbound;
let outbound;

self.onmessage = async (event) => {
  if (event.data.type === "configure") {
    ({ inbound, outbound } = event.data);
    return;
  }
  let state = 1;
  for (let batch = 0; batch < 20; batch += 1) {
    for (let index = 0; index < event.data.iterations; index += 1) {
      state = (state * 127 + 17) % 1000003;
    }
    outbound.postMessage("x");
    const received = await new Promise((resolve) => {
      inbound.onmessage = (message) => resolve(message.data);
    });
    state = (state + received.length) % 1000003;
  }
  self.postMessage(state);
};
