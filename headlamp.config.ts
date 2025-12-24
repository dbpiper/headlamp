const config = {
  sequential: true,
  coverage: {
    abortOnFailure: true,
    mode: "auto" as const,
    pageFit: true,
  },
  changed: {
    depth: 20,
  } as const,
};

export default config;
