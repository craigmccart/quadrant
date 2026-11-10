"""WinScan — Tool 6: Network Connections."""

import csv
import io
import subprocess
import threading

import customtkinter as ctk

from app.theme import (_font, ACCENT, ACCENT_HOVER, ON_ACCENT, GREEN, INFO,
                       INK, INK_DIM, INK_MUTE, R_BASE)
from app.widgets import make_page_header, make_tree, Tile


# ── Tool 6: Network Connections ──────────────────────────────────────────────

class NetworkTool(ctk.CTkFrame):

    def __init__(self, master):
        super().__init__(master, fg_color="transparent")
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(4, weight=1)
        self._loaded = False

        hdr, rule = make_page_header(
            self, title="Network",
            subtitle="Active TCP/UDP connections and owning processes.")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 10))

        # Action row
        actions = ctk.CTkFrame(self, fg_color="transparent")
        actions.grid(row=2, column=0, sticky="ew", pady=(0, 12))
        ctk.CTkButton(actions, text="Refresh",
                      width=100, height=34, font=_font(11, "bold"),
                      fg_color=ACCENT, hover_color=ACCENT_HOVER,
                      text_color=ON_ACCENT, corner_radius=R_BASE,
                      command=self._load).pack(side="left")
        self._status = ctk.CTkLabel(actions, text="Click Refresh to load connections.",
                                     font=_font(11), text_color=INK_DIM)
        self._status.pack(side="left", padx=14)

        # Compact 3-up stat strip inside ONE card (density over hero tiles)
        strip = Tile(self, "Connections")
        strip.frame.grid(row=3, column=0, sticky="ew", pady=(0, 12))
        self._stat_lbls = {}
        for i, (key, label) in enumerate([("total", "Total"),
                                          ("est", "Established"),
                                          ("listen", "Listening")]):
            strip.body.grid_columnconfigure(i, weight=1, uniform="net")
            col = ctk.CTkFrame(strip.body, fg_color="transparent")
            col.grid(row=0, column=i, sticky="w")
            val = ctk.CTkLabel(col, text="–", font=_font(20, "bold"),
                               text_color=INK, anchor="w")
            val.pack(anchor="w")
            ctk.CTkLabel(col, text=label, font=_font(11),
                         text_color=INK_DIM, anchor="w").pack(anchor="w")
            self._stat_lbls[key] = val

        # Table
        tree_frame, self._tree = make_tree(self, [
            ("Proto", 60), ("Local Address", 180), ("Remote Address", 180),
            ("State", 110), ("PID", 60), ("Process", 150)])
        tree_frame.grid(row=4, column=0, sticky="nsew")

        self._tree.tag_configure("established", foreground=GREEN)
        self._tree.tag_configure("listening", foreground=ACCENT)
        self._tree.tag_configure("time_wait", foreground=INK_MUTE)

    def _on_show(self):
        if not self._loaded:
            self._load()

    def _load(self):
        self._loaded = True
        self._tree.delete(*self._tree.get_children())
        threading.Thread(target=self._load_worker, daemon=True).start()

    def _load_worker(self):
        # Get PID -> process name map
        pid_map = {}
        try:
            r = subprocess.run(["tasklist", "/fo", "CSV", "/nh"],
                              capture_output=True, text=True, timeout=10,
                              creationflags=0x08000000)
            for row in csv.reader(io.StringIO(r.stdout)):
                if len(row) >= 2:
                    try:
                        pid_map[row[1].strip()] = row[0].strip().strip('"')
                    except (ValueError, IndexError):
                        pass
        except Exception:
            pass

        # Parse netstat
        connections = []
        try:
            r = subprocess.run(["netstat", "-ano"],
                              capture_output=True, text=True, timeout=10,
                              creationflags=0x08000000)
            for line in r.stdout.splitlines():
                parts = line.split()
                if len(parts) >= 4 and parts[0] in ("TCP", "UDP"):
                    proto = parts[0]
                    local = parts[1]
                    if proto == "TCP":
                        remote = parts[2]
                        state = parts[3] if len(parts) > 3 else ""
                        pid = parts[4] if len(parts) > 4 else ""
                    else:
                        remote = parts[2] if len(parts) > 2 and ":" in parts[2] else "*:*"
                        state = ""
                        pid = parts[-1]
                    proc = pid_map.get(pid, "")
                    connections.append((proto, local, remote, state, pid, proc))
        except Exception:
            pass

        self.after(0, lambda: self._on_done(connections))

    def _on_done(self, connections):
        est_count = 0
        listen_count = 0
        for proto, local, remote, state, pid, proc in connections:
            tag = ""
            if "ESTABLISHED" in state:
                tag = "established"
                est_count += 1
            elif "LISTENING" in state or "LISTEN" in state:
                tag = "listening"
                listen_count += 1
            elif "TIME_WAIT" in state or "CLOSE_WAIT" in state:
                tag = "time_wait"
            self._tree.insert("", "end", values=(proto, local, remote, state, pid, proc),
                             tags=(tag,) if tag else ())
        # Count lives in the stat strip only — no duplicate next to Refresh.
        self._status.configure(text="Up to date.", text_color=INK_DIM)
        self._stat_lbls["total"].configure(text=str(len(connections)))
        self._stat_lbls["est"].configure(text=str(est_count))
        self._stat_lbls["listen"].configure(text=str(listen_count))
