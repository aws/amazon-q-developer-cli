import { useEffect, useState } from "react";

// type Cargo = {
//   authors: string
//   description: string
//   license: string
//   license_file: null
//   name: string
//   repository: string
//   version: string
// }

// type Npm = {
//   author?: string
//   description: string
//   homepage: string
//   license: string
//   name: string
//   path: string
//   version: string
// }

export default function Page() {
  const [license, setLicense] = useState<string | undefined>();
  // const [npm, setNpm] = useState<Npm[]>([])
  // const [cargo, setCargo] = useState<Cargo[]>([])

  // useEffect(() => {
  //   fetch('/assets/license/npm.json')
  // .then(response => {
  //     if (!response.ok) {
  //         throw new Error("HTTP error " + response.status);
  //     }
  //     return response.json();
  // })
  // .then(json => {

  //   // @ts-expect-error whining about item
  //   const flatJson = [...Object.entries(json).map(e => e[1]).flat()].filter((item) => item.name.includes('aws-'))
  //   console.log({ npm: flatJson })
  //   // @ts-expect-error idk why it's mad about this... they're all the same type.
  //   setNpm(flatJson)
  // })
  // .catch((e) => {
  //     console.error(e)
  // })
  // }, [])

  useEffect(() => {
    fetch("/assets/license/NOTICE.txt")
      .then((response) => {
        if (!response.ok) {
          throw new Error("HTTP error " + response.status);
        }
        return response.text();
      })
      .then((text) => {
        setLicense(text);
      })
      .catch((e) => {
        console.error(e);
      });
  }, []);

  return (
    <>
      <section className={`flex flex-col py-4`}>
        <h2
          id={`subhead-licenses`}
          className="font-bold text-medium text-zinc-400 leading-none mt-2"
        >
          Licenses
        </h2>
        <div className={`flex p-4 pl-0 gap-4 whitespace-pre-wrap`}>
          {license}
        </div>
      </section>
    </>
  );
}
