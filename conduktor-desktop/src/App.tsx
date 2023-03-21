import { useState } from "react";
import reactLogo from "./assets/logo.png";
import { invoke } from "@tauri-apps/api/tauri";
import "./App.css";

function App() {
  const [remotePort, setRemotePort] = useState(0);
  const [host, setHost] = useState("");
  const [port, setPort] = useState("");

  async function start_proxy() {
    // Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
    console.log("start_proxy", host, port);
    const rport: number = await invoke("start_proxy", { host, port });
    console.log(rport);
    setRemotePort(rport);
  }

  return (
    <div className="container">
      <h1>Welcome to the conduktor proxy manager</h1>

      <div className="row">
        <a href="https://reactjs.org" target="_blank">
          <img src={reactLogo} className="logo react" alt="React logo" />
        </a>
      </div>

      <p>Click on the Tauri, Vite, and React logos to learn more.</p>

      <div className="row">
        <form
          onSubmit={(e) => {
            e.preventDefault();
            start_proxy();
          }}
        >
          <input
            id="host-input"
            onChange={(e) => setHost(e.currentTarget.value)}
            placeholder="Enter a host..."
          />     
               <input
            id="port-input"
            onChange={(e) => setPort(e.currentTarget.value)}
            placeholder="Enter a port..."
          />
          <button type="submit">Launch the proxy</button>
        </form>
      </div>
      <p>{remotePort}</p>
    </div>
  );
}

export default App;
