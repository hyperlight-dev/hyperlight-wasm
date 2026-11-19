import host from "example:hello/host";

export const guest = {
  greet(name) {
    const message = `JavaScript guest says hello to ${name}`;
    host.print(message);
    return `Hello, ${name}!`;
  },

  describeWords(words) {
    const longest = words.reduce(
      (candidate, word) => (word.length > candidate.length ? word : candidate),
      "",
    );

    return {
      count: words.length,
      longest,
      joined: words.map((word) => word.toUpperCase()).join(" | "),
    };
  },

  invoiceTotal(amounts, taxRate) {
    if (amounts.length === 0) {
      throw "invoice must contain at least one amount";
    }
    if (taxRate < 0) {
      throw "tax rate must be non-negative";
    }

    const subtotal = amounts.reduce((sum, amount) => sum + amount, 0);
    const tax = Math.round(subtotal * taxRate * 100) / 100;

    return {
      subtotal,
      tax,
      total: subtotal + tax,
    };
  },
};
