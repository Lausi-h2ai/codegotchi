import "./App.css";

function App() {
    return (
        <main className="app-shell">
            <header className="hero">
                <p className="eyebrow">Phase 1 · workspace foundation</p>
                <h1>CodeGotchi</h1>
                <p className="hero-copy">
                    A small home for a coding companion. The room below is a
                    deliberately simple visual starting point.
                </p>
            </header>

            <section className="room-card" aria-labelledby="room-title">
                <div className="room-card__header">
                    <div>
                        <p className="section-kicker">Current view</p>
                        <h2 id="room-title">Phase 1 placeholder room</h2>
                    </div>
                    <span className="status-pill">Static preview</span>
                </div>

                <div
                    className="room-illustration"
                    role="img"
                    aria-label="A geometric CodeGotchi pet beside a desk and bowl"
                >
                    <div className="room-window" aria-hidden="true">
                        <span className="window-pane window-pane--vertical" />
                        <span className="window-pane window-pane--horizontal" />
                        <span className="window-sun" />
                    </div>

                    <div className="room-shelf" aria-hidden="true">
                        <span className="shelf-book shelf-book--one" />
                        <span className="shelf-book shelf-book--two" />
                        <span className="shelf-plant" />
                    </div>

                    <div className="desk" aria-hidden="true">
                        <span className="desk-top" />
                        <span className="desk-leg desk-leg--left" />
                        <span className="desk-leg desk-leg--right" />
                        <span className="desk-drawer" />
                    </div>

                    <div className="pet" aria-hidden="true">
                        <span className="pet-ear pet-ear--left" />
                        <span className="pet-ear pet-ear--right" />
                        <span className="pet-body" />
                        <span className="pet-face">
                            <span className="pet-eye pet-eye--left" />
                            <span className="pet-eye pet-eye--right" />
                            <span className="pet-smile" />
                        </span>
                        <span className="pet-foot pet-foot--left" />
                        <span className="pet-foot pet-foot--right" />
                    </div>

                    <div className="bowl" aria-hidden="true">
                        <span className="bowl-food" />
                    </div>
                </div>

                <p className="room-note">
                    Static HTML and CSS geometry only. Authoritative pet state,
                    care interactions, and synchronization arrive in later
                    slices.
                </p>
            </section>
        </main>
    );
}

export default App;
