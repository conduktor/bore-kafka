import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { useAuth, withAuthenticationRequired } from "@conduktor/auth";
import { SpinnerDots } from "@conduktor/ui-library";
import "./App.css";

const App = withAuthenticationRequired(AppMain, {
  onRedirecting: () => <SpinnerDots />,
})

function AppMain() {
  const auth = useAuth();
  //const orgs = useOrganizations();

  useEffect(() => {
    async function forwardToken() {
      const token = await auth.getAccessTokenSilently();
      await invoke("token", {token: token.access_token});
      // const slug = orgs.currentOrganization?.slug ?? 'conduktor';
      // console.log(`Redirecting to https://${slug}.stg.conduktor.app/`);
      window.location.href = "https://signup.staging.conduktor.io/";
    }

    forwardToken();
  }, [auth]);

  return (
    <SpinnerDots />
  );
}

export default App;
