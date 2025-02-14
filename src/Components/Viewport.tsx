import { ActionIcon, Box, Button, Center, Flex, Grid } from "@mantine/core";
import { useViewportSize } from "@mantine/hooks";
import { IconPhoto, IconSettings, IconHeart, IconEdit, IconTrashX, IconEyeOff, IconAdjustments, IconPlus } from '@tabler/icons-react';
import Clip from "./Clip";
import { useEffect } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import FastClip from "../Classes/FastClip";






export default function ListOfClips() {

    const clip = new FastClip("Hello World", "Greeting", "📌", 60);



    const buttonLabels = [clip, clip];

    const { height } = useViewportSize();


    useEffect(() => {
        const unlistenPromises: Promise<UnlistenFn>[] = [];

        unlistenPromises.push(listen('basler_update_get', (event) => {}));
   
        return () => {
            // Unlisten to all events
            unlistenPromises.forEach((unlistenPromise) => {
                unlistenPromise.then((unlisten) => unlisten());
            });
        };
    }, []);





    return (

        <Flex
            w="100%"

            maw="500px"
            gap="md"
            align="center"
            direction="column"
            style={{ height: `${height}px` }}
            p={10}
        >
            {buttonLabels.map((clip, index) => (
                <Clip key={index} fast_clip={clip} />
            ))}


        </Flex>

    )
}
