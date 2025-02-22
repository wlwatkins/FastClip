import { Center } from "@mantine/core";
import ListOfClips from "./Components/Viewport";
import "./App.css";
import "@mantine/core/styles.css"; 
import '@mantine/notifications/styles.css';
import New from "./Components/NewClip";
import ToolBar from "./Components/ToolBar";
import Settings from "./Components/SettingsClip";

function App() {
  return (
    <Center  className="bg-zinc-800">
      <ToolBar/>
      <ListOfClips/>
      <New/>
      <Settings/>
    </Center>
  );
}

export default App;
