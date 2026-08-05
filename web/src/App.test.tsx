import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import App from "./App";

describe("CodeGotchi room shell", () => {
    it("renders an accessible heading and a labeled Phase 1 placeholder room", () => {
        render(<App />);

        expect(
            screen.getByRole("heading", { level: 1, name: "CodeGotchi" }),
        ).toBeInTheDocument();
        expect(
            screen.getByRole("region", { name: "Phase 1 placeholder room" }),
        ).toBeInTheDocument();
        expect(
            screen.getByText("Phase 1 placeholder room"),
        ).toBeInTheDocument();
    });
});
